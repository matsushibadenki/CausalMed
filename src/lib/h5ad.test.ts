import { afterEach, beforeEach, expect, it, vi } from 'vitest';

const mock = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
  subscribe: vi.fn(),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: mock.invoke, isTauri: () => true }));
vi.mock('@tauri-apps/api/event', () => ({ listen: mock.listen }));
vi.mock('react', () => ({ useSyncExternalStore: mock.subscribe }));

let invalidate: () => void;
let unsubscribe: () => void;
let getSnapshot: () => unknown;
const unlisten = vi.fn();

beforeEach(() => {
  vi.resetModules();
  vi.resetAllMocks();
  vi.useFakeTimers();
  vi.stubGlobal('window', new EventTarget());
  mock.invoke.mockResolvedValue({ status: 'idle' });
  mock.listen.mockImplementation(async (_event: string, handler: () => void) => {
    invalidate = handler;
    return unlisten;
  });
  mock.subscribe.mockImplementation((subscribe: (fn: () => void) => () => void, read: () => unknown) => {
    getSnapshot = read;
    unsubscribe = subscribe(() => {});
  });
});

afterEach(() => {
  unsubscribe();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

it('fetches on invalidation and polls only while the worker is active', async () => {
  const { useH5ad } = await import('./h5ad');
  useH5ad();
  await vi.advanceTimersByTimeAsync(3000);
  expect(mock.listen).toHaveBeenCalledWith('h5ad-state-changed', expect.any(Function));
  expect(mock.invoke).toHaveBeenCalledTimes(1);

  mock.invoke.mockResolvedValueOnce({ status: 'running', projectKey: 'project-a', id: 4, report: { cells: 300 } });
  invalidate();
  await vi.advanceTimersByTimeAsync(149);
  expect(mock.invoke).toHaveBeenCalledTimes(2);
  mock.invoke.mockResolvedValueOnce({ status: 'running', percent: 75 });
  await vi.advanceTimersByTimeAsync(1);
  expect(mock.invoke).toHaveBeenLastCalledWith('get_h5ad_progress', { projectKey: 'project-a', id: 4 });
  expect(getSnapshot()).toMatchObject({ percent: 75, report: { cells: 300 } });
  expect(mock.invoke).toHaveBeenCalledTimes(3);

  mock.invoke.mockResolvedValueOnce(null);
  await vi.advanceTimersByTimeAsync(3000);
  expect(mock.invoke).toHaveBeenCalledTimes(5);
  expect(mock.invoke).toHaveBeenLastCalledWith('get_h5ad_state');
  expect(getSnapshot()).toMatchObject({ status: 'idle' });

  unsubscribe();
  expect(unlisten).toHaveBeenCalledOnce();
  invalidate();
  await vi.advanceTimersByTimeAsync(3000);
  expect(mock.invoke).toHaveBeenCalledTimes(5);
});

it('retains idle polling when the event registration fails', async () => {
  mock.listen.mockRejectedValueOnce(new Error('unavailable'));
  const { useH5ad } = await import('./h5ad');
  useH5ad();
  await vi.advanceTimersByTimeAsync(2000);
  expect(mock.invoke).toHaveBeenCalledTimes(3);
});

it('recovers a failed idle snapshot without waiting for another event', async () => {
  mock.invoke.mockRejectedValueOnce(new Error('temporary IPC failure'));
  const { useH5ad } = await import('./h5ad');
  useH5ad();
  await vi.advanceTimersByTimeAsync(3000);
  expect(mock.invoke.mock.calls).toEqual([['get_h5ad_state'], ['get_h5ad_state']]);
  expect(getSnapshot()).toEqual({ status: 'idle' });
});
