# Feature-space matching and numeric categories

Scalar outcome inspection reads feature IDs in 8,192-row blocks. A private temporary SQLite database with a 1 MiB page cache checks uniqueness and supplies the sorted IDs for the existing JSON SHA-256. The target retains its source-column position. Missing and ambiguous targets retain precedence over feature-space validity errors. Unicode, escaping, whitespace handling and the persisted digest format are covered by compatibility tests.

Large integer and floating-point dictionaries validate uniqueness in blocks and read only the codes referenced by each row block. Numeric keys retain exact precision, including unsigned IDs beyond the signed 64-bit range. Floating-point validation treats signed zero as one value and rejects NaN categories. Legacy tables and unsupported dictionary encodings use the existing decode path. Temporary disk usage scales with distinct labels. Individual variable-length values and downstream per-cell arrays remain outside these bounds.

| Synthetic workload | Additional peak RSS, full | Additional peak RSS, blocked |
| --- | ---: | ---: |
| 524,288 feature IDs, matching and digest | 23.4 MiB | 11.1 MiB |
| 524,288 unsigned integer categories, 16,384 referenced | 48.9 MiB | 31.8 MiB |

Fresh-process probes produced identical feature-space and selection-mask hashes respectively. These measurements isolate the changed operations.

Independent AnnData calculations on Kang 2018 and its 12,859-cell derived H5AD matched the position of ISG15 and the existing 15,706-feature digest `8f4f358e5bf0a35a5a7be9f52ea8f61b56a01aacb89a9f7d2fc6e064c3409737`. A temporary copy of the derived H5AD with an added QA-only unsigned 64-bit categorical feature column matched the full decode for both the selected position and the digest. Those identifiers were generated for the storage test.

The supervised Rust integration confirmed eight donor summaries, eligibility abstention, portable restore, SHA-based reconnection and project isolation.
