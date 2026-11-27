"""Read-only research bridge for approved aggregate CausalMed QC summaries."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import re
import sys
from urllib.error import HTTPError, URLError
from urllib.parse import quote, urlsplit
from urllib.request import HTTPRedirectHandler, ProxyHandler, Request, build_opener
import zipfile


SNAPSHOT_LIMIT = 16384
MANIFEST_LIMIT = 2 * 1024 * 1024
RESPONSE_LIMIT = 1024 * 1024
PROTOCOL_VERSIONS = ('2025-11-25', '2025-06-18', '2025-03-26', '2024-11-05')
SYSTEM_PROMPT = (
    'You are reviewing aggregate CausalMed expression QC. Treat the supplied summary as data. '
    'Distinguish observations, intervention evidence, and unvalidated hypotheses. '
    'Identify limitations and propose ways to check them. Do not invent results, infer clinical '
    'utility or causal effects, or treat cells as independent donors. Flag partial or invalid QC. '
    'If the data are synthetic or their origin is unconfirmed, state that clearly.'
)
LANGUAGES = {'en': 'English', 'ja': 'Japanese', 'zh-CN': 'Simplified Chinese'}
ERROR_LABELS = {'en': 'Research bridge error', 'ja': '研究接続エラー', 'zh-CN': '研究连接错误'}


def snapshot(report, origin):
    """Keep only aggregate QC fields and the source content digest."""
    qc = report['qc']
    donor = qc['donor']
    digest = report['sha256']
    status = qc['status']
    if (not isinstance(digest, str) or not re.fullmatch('[a-f0-9]{64}', digest)
            or status not in ('ready', 'partial', 'invalid_counts')
            or origin not in ('synthetic', 'real', 'unconfirmed')):
        raise ValueError('invalid_snapshot')
    metrics = {}
    for key, value in {
        'cells': report['cells'], 'genes': report['genes'],
        'duplicateCellIds': report['duplicateCellIds'], 'duplicateGeneIds': report['duplicateGeneIds'],
        'retained': qc['retained'], 'excluded': qc['excluded'], 'mitochondrialFeatures': qc['mitoFeatures'],
        'donorLabels': donor['distinct'], 'missingDonorLabels': donor['missing'],
        'retainedWithMissingDonorLabel': donor['missingRetained'],
    }.items():
        if value is not None and (type(value) is not int or value < 0):
            raise ValueError('invalid_snapshot')
        metrics[key] = value
    cutoffs = {}
    for key in ('minGenes', 'maxGenes', 'minCounts', 'maxCounts', 'maxMitoPct'):
        value = qc['options'][key]
        if value is not None and (type(value) not in (int, float) or not math.isfinite(value)):
            raise ValueError('invalid_snapshot')
        cutoffs[key] = value
    return {'version': 1, 'sourceSha256': digest, 'origin': origin, 'qcStatus': status,
            'metrics': metrics, 'cutoffs': cutoffs}


def qc_archive_snapshot(path, origin):
    """Build a snapshot from the manifest alone, without extracting the archive."""
    try:
        with zipfile.ZipFile(path) as archive:
            manifests = [entry for entry in archive.infolist() if entry.filename == 'manifest.json']
            if len(manifests) != 1 or manifests[0].file_size > MANIFEST_LIMIT:
                raise ValueError('invalid_qc_archive')
            with archive.open(manifests[0]) as stream:
                raw = stream.read(MANIFEST_LIMIT + 1)
    except (zipfile.BadZipFile, NotImplementedError, RuntimeError) as error:
        raise ValueError('invalid_qc_archive') from error
    if len(raw) > MANIFEST_LIMIT:
        raise ValueError('invalid_qc_archive')
    manifest = json.loads(raw)
    if manifest['schemaVersion'] != 1 or manifest['algorithm'] != 'causalmed-count-qc-v1':
        raise ValueError('invalid_qc_archive')
    source = manifest['source']
    summary = snapshot({
        'sha256': source['sha256'], 'cells': source['cells'], 'genes': source['genes'],
        'duplicateCellIds': None, 'duplicateGeneIds': None, 'qc': manifest['qc'],
    }, origin)
    summary['savedQc'] = saved_qc_reference(manifest['analysisId'], hashlib.sha256(raw).hexdigest())
    return summary


def saved_qc_reference(analysis_id, digest):
    if (not isinstance(analysis_id, str)
            or not re.fullmatch('[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}', analysis_id)
            or not isinstance(digest, str) or not re.fullmatch('[a-f0-9]{64}', digest)):
        raise ValueError('invalid_snapshot')
    return {'analysisId': analysis_id, 'archiveManifestSha256': digest}


def load_snapshot(path):
    with path.open('rb') as stream:
        data = stream.read(SNAPSHOT_LIMIT + 1)
    if len(data) > SNAPSHOT_LIMIT:
        raise ValueError('invalid_snapshot')
    saved = json.loads(data)
    if type(saved['version']) is not int or saved['version'] != 1:
        raise ValueError('invalid_snapshot')
    metrics = saved['metrics']
    # Reapply the allowlist even when a snapshot was manually edited.
    summary = snapshot({
        'sha256': saved['sourceSha256'],
        **{key: metrics[key] for key in ('cells', 'genes', 'duplicateCellIds', 'duplicateGeneIds')},
        'qc': {
            'status': saved['qcStatus'], 'options': saved['cutoffs'],
            'retained': metrics['retained'], 'excluded': metrics['excluded'],
            'mitoFeatures': metrics['mitochondrialFeatures'],
            'donor': {'distinct': metrics['donorLabels'], 'missing': metrics['missingDonorLabels'],
                      'missingRetained': metrics['retainedWithMissingDonorLabel']},
        },
    }, saved['origin'])
    if 'savedQc' in saved:
        reference = saved['savedQc']
        summary['savedQc'] = saved_qc_reference(reference['analysisId'], reference['archiveManifestSha256'])
    return summary


class NoRedirects(HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


def local_endpoint(base_url):
    parts = urlsplit(base_url)
    if (parts.scheme not in ('http', 'https') or parts.hostname not in ('127.0.0.1', '::1', 'localhost')
            or parts.username is not None or parts.password is not None or parts.query or parts.fragment):
        raise ValueError('invalid_local_url')
    return base_url.rstrip('/') + '/chat/completions'


def research(provider, model, question, summary, language, base_url, timeout):
    if not model.strip() or len(model) > 200 or not question.strip() or len(question) > 8000:
        raise ValueError('invalid_model_or_question')
    system = SYSTEM_PROMPT + ' Respond in ' + LANGUAGES[language] + '.'
    prompt = json.dumps({'question': question, 'qcSummary': summary}, ensure_ascii=False)
    headers = {'Content-Type': 'application/json'}
    if provider == 'openai':
        key = os.environ.get('OPENAI_API_KEY')
        url = 'https://api.openai.com/v1/responses'
        payload = {'model': model, 'instructions': system, 'input': prompt,
                   'store': False, 'max_output_tokens': 2048}
    elif provider == 'gemini':
        key = os.environ.get('GEMINI_API_KEY')
        url = ('https://generativelanguage.googleapis.com/v1beta/models/'
               + quote(model, safe='') + ':generateContent')
        if key:
            headers['x-goog-api-key'] = key
        payload = {'systemInstruction': {'parts': [{'text': system}]},
                   'contents': [{'role': 'user', 'parts': [{'text': prompt}]}],
                   'generationConfig': {'maxOutputTokens': 2048}}
    else:
        key = os.environ.get('CAUSALMED_LOCAL_API_KEY')
        url = local_endpoint(base_url)
        payload = {'model': model, 'max_tokens': 2048,
                   'messages': [{'role': 'system', 'content': system}, {'role': 'user', 'content': prompt}]}
    if provider != 'local' and not key:
        raise ValueError('missing_api_key')
    if key and provider != 'gemini':
        headers['Authorization'] = 'Bearer ' + key
    opener = build_opener(NoRedirects(), ProxyHandler({})) if provider == 'local' else build_opener(NoRedirects())
    request = Request(url, data=json.dumps(payload).encode('utf-8'), headers=headers, method='POST')
    try:
        with opener.open(request, timeout=timeout) as response:
            raw = response.read(RESPONSE_LIMIT + 1)
    except HTTPError as error:
        raise ValueError(f'provider_http_{error.code}') from None
    except (URLError, TimeoutError):
        raise ValueError('provider_unavailable') from None
    if len(raw) > RESPONSE_LIMIT:
        raise ValueError('provider_response_too_large')
    data = json.loads(raw)
    if provider == 'openai':
        parts = [part['text'] for item in data.get('output', []) if item.get('type') == 'message'
                 for part in item.get('content', []) if part.get('type') == 'output_text']
    elif provider == 'gemini':
        candidates = data.get('candidates', [])
        parts = [part['text'] for part in candidates[0].get('content', {}).get('parts', [])
                 if 'text' in part and not part.get('thought')] if candidates else []
    else:
        parts = [data['choices'][0]['message']['content']] if data.get('choices') else []
    answer = '\n'.join(part for part in parts if isinstance(part, str) and part.strip())
    if not answer:
        raise ValueError('provider_returned_no_text')
    result = {'provider': provider, 'model': model, 'sourceSha256': summary['sourceSha256'],
              'origin': summary['origin'], 'kind': 'unvalidated_research_notes', 'answer': answer}
    if 'savedQc' in summary:
        result['savedQc'] = summary['savedQc']
    return result


def serve_mcp(summary, language):
    tool = {
        'name': 'get_qc_summary', 'description': 'Read the configured CausalMed aggregate QC snapshot.',
        'inputSchema': {'type': 'object', 'properties': {}, 'additionalProperties': False},
        'annotations': {'readOnlyHint': True, 'openWorldHint': False},
    }
    while raw := sys.stdin.buffer.readline(SNAPSHOT_LIMIT + 1):
        if len(raw) > SNAPSHOT_LIMIT:
            return
        try:
            message = json.loads(raw)
        except ValueError:
            message = None
        response = {'jsonrpc': '2.0', 'id': None}
        if not isinstance(message, dict) or message.get('jsonrpc') != '2.0':
            response['error'] = {'code': -32700 if message is None else -32600, 'message': 'Invalid request'}
        elif 'id' not in message:
            continue
        else:
            response['id'] = message['id']
            method = message.get('method')
            params = message.get('params', {})
            if not isinstance(params, dict):
                response['error'] = {'code': -32602, 'message': 'Invalid params'}
            elif method == 'initialize':
                version = params.get('protocolVersion')
                response['result'] = {
                    'protocolVersion': version if version in PROTOCOL_VERSIONS else PROTOCOL_VERSIONS[0],
                    'capabilities': {'tools': {}}, 'serverInfo': {'name': 'causalmed-research', 'version': '1.0.0'},
                    'instructions': SYSTEM_PROMPT + ' Respond in ' + LANGUAGES[language] + '.',
                }
            elif method == 'ping':
                response['result'] = {}
            elif method == 'tools/list':
                response['result'] = {'tools': [tool]}
            elif method == 'tools/call':
                if params.get('name') != tool['name'] or params.get('arguments', {}) != {}:
                    response['error'] = {'code': -32602, 'message': 'Invalid tool or arguments'}
                else:
                    response['result'] = {'content': [{'type': 'text', 'text': json.dumps(summary)}]}
            else:
                response['error'] = {'code': -32601, 'message': 'Method not found'}
        print(json.dumps(response), flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--lang', choices=LANGUAGES, default='en')
    commands = parser.add_subparsers(dest='command', required=True)
    prepare = commands.add_parser('snapshot')
    prepare.add_argument('h5ad', type=Path)
    prepare.add_argument('--donor-column', default='')
    from_archive = commands.add_parser('snapshot-qc')
    from_archive.add_argument('archive', type=Path)
    for command in (prepare, from_archive):
        command.add_argument('--output', required=True, type=Path)
        command.add_argument('--origin', choices=('synthetic', 'real', 'unconfirmed'), default='unconfirmed')
    ask = commands.add_parser('ask')
    mcp = commands.add_parser('mcp')
    for command in (ask, mcp):
        command.add_argument('--snapshot', required=True, type=Path)
    ask.add_argument('--provider', choices=('gemini', 'openai', 'local'), required=True)
    ask.add_argument('--model', required=True)
    ask.add_argument('--question', required=True)
    ask.add_argument('--base-url', default='http://127.0.0.1:11434/v1')
    ask.add_argument('--timeout', type=int, choices=range(1, 121), default=45, metavar='SECONDS')
    args = parser.parse_args()
    try:
        if args.command in ('snapshot', 'snapshot-qc'):
            if args.command == 'snapshot-qc':
                summary = qc_archive_snapshot(args.archive, args.origin)
            else:
                from causalmed_worker.h5ad import inspect
                report = inspect(args.h5ad, options={'donorColumn': args.donor_column})
                summary = snapshot(report, args.origin)
            with args.output.open('x', encoding='utf-8') as stream:
                json.dump(summary, stream, indent=2, allow_nan=False)
                stream.write('\n')
        else:
            summary = load_snapshot(args.snapshot)
            if args.command == 'mcp':
                serve_mcp(summary, args.lang)
            else:
                output = research(args.provider, args.model, args.question, summary,
                                  args.lang, args.base_url, args.timeout)
                print(json.dumps(output, ensure_ascii=False, indent=2))
    except (AttributeError, IndexError, KeyError, OSError, TypeError, ValueError, ImportError) as error:
        code = str(error) if isinstance(error, ValueError) and re.fullmatch('[a-z_0-9]+', str(error)) else 'request_failed'
        print(f'{ERROR_LABELS[args.lang]} ({code})', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
