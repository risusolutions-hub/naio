# Niao Library Registry

Canonical index of all **202** libraries in [`niao_libs/`](.). Each one-line summary comes from that library's `package.json` `description` field.

- **Default runtime version:** `0.2.3` (most native builtins; `ahiru` is `0.3.0`)
- **Install:** `nm install <lib>` · **Browse:** [nms.taurus-tech.in](https://nms.taurus-tech.in)
- **Source catalog:** [`catalog.json`](catalog.json) — core packages shipped with `nm install --global`

> Auto-generated from `niao_libs/*/package.json` on 2026-07-22. Regenerate: `python niao_libs/_gen_registry.py`

---

## Contents

- [Core & builtins](#core-builtins)
- [I/O, formats & encoding](#i/o-formats-encoding)
- [Networking & web](#networking-web)
- [Databases & storage](#databases-storage)
- [Data & analytics](#data-analytics)
- [Math & numerics](#math-numerics)
- [Machine learning](#machine-learning)
- [AI & LLM](#ai-llm)
- [Cloud & integrations](#cloud-integrations)
- [System & runtime](#system-runtime)
- [Strings, validation & utilities](#strings-validation-utilities)
- [Developer tools & testing](#developer-tools-testing)
- [Applications & frameworks](#applications-frameworks)
- [Other](#other)
- [Alphabetical index](#alphabetical-index)

---

## Core & builtins

| Library | Description |
|---------|-------------|
| **bignum** | Arbitrary-precision integer arithmetic |
| **collections** | Insertion-ordered maps/sets (IndexMap/IndexSet) and fast hashing |
| **core** | Core builtins: print, len, type, assert, errors, timing, arrays |
| **dsa** | Data structures and algorithms: list, stack, queue, heap, map, graph, sort |
| **narena** | Pooled packed-buffer reuse arena for GC-pressure relief |
| **nasync** | Structured async ergonomics over io tasks: gather/race, timeouts, cancellation, async channels. ~asyncio, trio subset |
| **niter** | Iterator & combinatorics toolkit: product, permutations, combinations, groupby, windows, chunked, flatten, zip_longest. ~itertools, more-itertools (general values; complements nlazy packed pipelines) ... |
| **nlazy** | Fused lazy map/filter/take pipelines over packed arrays |
| **npar** | Explicit rayon parallel ops on packed arrays with set_threads |
| **npersist** | im-rc persistent Vector/HashMap with structural sharing |
| **npipe** | Typed step pipelines over built-in ops (id, len, type, keys, not_nil, str, abs) |
| **nproc** | Child processes beyond nshell: process pools, pipes, IPC channels, shared memory. ~multiprocessing |
| **nrand** | Fast random numbers (xoshiro256**): ints, floats, strings, choice/shuffle/sample, distributions |
| **nsimd** | Unrolled autovectorized f64/i64 kernels on packed FloatArray/IntArray |
| **nsorted** | Sorted list / dict / set with bisect insert, range queries, nearest lookup. ~sortedcontainers, bisect ... |
| **parallel** | Threading, mutexes, channels, worker pools, and cooperative poll |
| **rand** | Seeded PRNG, uniform ranges, shuffle, and choose |
| **re** | Regular expressions: match, find, replace, split |

## I/O, formats & encoding

| Library | Description |
|---------|-------------|
| **archive** | Gzip and deflate compression helpers |
| **codec** | Base64, hex, UUID, and dotenv helpers |
| **io** | File I/O, paths, streaming handles, async tasks, raw socket options (niao_io) |
| **json** | JSON parse, stringify, and object utilities |
| **nbinary** | Binary data: struct pack/unpack, endianness, bit fields, varints, CRC32/64. ~struct, bitstring ... |
| **ncal** | Calendar math: business days, holiday tables, week numbers, month grids. ~calendar, holidays, workalendar ... |
| **ncanon** | Canonical JSON-like encoding, FNV-1a hash, structural equal, fingerprint |
| **ncbor** | CBOR encode/decode (IoT / COSE friendly). ~cbor2 ... |
| **ncolumnar** | Column-major binary codec for tables (magic NCOL1) |
| **ncsv** | Lightweight CSV parse/stringify and file read/write |
| **nencoding** | Charset detection + transcoding: UTF-16, Shift-JIS, GBK, Latin-1, BOM handling. ~codecs, charset-normalizer... |
| **nfs** | High-level file ops: copy/move trees, atomic write, temp files/dirs, disk usage, trash. ~shutil, tempfile, send2trash (above io handles / nos basics) |
| **ngeo** | Geospatial: haversine, GeoJSON, points/polygons, bounding boxes, tile math. ~shapely, geopy, geojson |
| **nglob** | Glob patterns, ** recursion, gitignore-style matching, walk with filters. ~glob, fnmatch, pathspec... |
| **nhdf5** | HDF5 scientific dataset read/write, groups, attrs. ~h5py ... |
| **nhtml** | Forgiving HTML5 parser, CSS selectors, tree walking, text extraction, escape/unescape. ~BeautifulSoup4 ... |
| **nical** | iCalendar / vCard parse + generate, recurrence rules (RRULE). ~icalendar, vobject ... |
| **nipaddr** | IPv4/IPv6 addresses, CIDR networks, ranges, subnet math, membership checks. ~ipaddress... |
| **njpath** | JSONPath / JMESPath-style queries, JSON Pointer and JSON Patch over values. ~jmespath, jsonpath-ng, glom ... |
| **nmarkdown** | Lightweight Markdown to HTML, plain-text strip, and heading extraction |
| **nmime** | File-type detection by magic bytes, extension<->MIME maps. ~python-magic, filetype ... |
| **nmmap** | Memory-mapped files via memmap2, lazy line index, byte search |
| **nmsgpack** | MessagePack encode/decode, streaming. ~msgpack... |
| **nparquet** | Parquet + Arrow IPC read/write, nframe interop for data interchange. ~pyarrow (beside ncolumnar's NCOL1) ... |
| **npdf** | PDF create (text, images, tables) + extract text and pages, merge/split. ~reportlab, pypdf ... |
| **nproto** | Protocol Buffers codec + codegen from .proto files. ~protobuf ... |
| **nsnap** | Fast binary value snapshots with fingerprints and staleness checks (NSNP1) |
| **ntar** | tar archives read/write incl. .tar.gz / .tar.zst. ~tarfile ... |
| **ntoml** | TOML parse and stringify for configuration files |
| **nurl** | URL parse, build, join, query helpers, and percent encoding |
| **nview** | Jinja-style templating: inheritance, blocks, filters, autoescape, partials -- for HTML/text output. ~jinja2 (distinct from ntemplate's LLM prompt templates) |
| **nxlsx** | Excel .xlsx read/write: sheets, styles, formulas, streaming rows. ~openpyxl, xlsxwriter ... |
| **nyaml** | YAML 1.2 parse + emit, safe-by-default, anchors, multi-doc. ~PyYAML, ruamel.yaml... |
| **nzip** | ZIP archives: read/write, streaming, per-entry compression, encryption. ~zipfile ... |
| **time** | Wall clock, formatting, parsing, time zones, and date arithmetic |

## Networking & web

| Library | Description |
|---------|-------------|
| **crypto** | SHA-256/512 and HMAC helpers |
| **http** | HTTP types: Method, StatusCode, HeaderMap, Uri |
| **log** | Structured logging, level filtering, and spans |
| **nauth** | Web auth kit: sessions, login/logout, password reset, RBAC roles, CSRF tokens. ~flask-login, django auth (built on npass + nsign) |
| **ncrypt** | Modern crypto: AES-GCM, ChaCha20-Poly1305, RSA, Ed25519/X25519, HKDF/PBKDF2, X.509 parse, CSPRNG, constant-time compare. ~cryptography, pynacl, secrets (extends crypto's SHA/HMAC) ... |
| **net** | Networking: HTTP, TCP/UDP, DNS, TLS, WebSocket, SMTP, FTP |
| **net_clients** | SMTP and FTP client helpers |
| **ngraphql** | GraphQL client (queries, variables, fragments) + schema/server helpers. ~gql, graphene, strawberry ... |
| **nimap** | IMAP4 + POP3 mailbox retrieval: search, flags, folders, IDLE push. ~imaplib, imapclient ... |
| **njwt** | JWT / JWS sign + verify (HS/RS/ES/EdDSA), claims and expiry validation, JWKS fetch. ~pyjwt, python-jose ... |
| **nlog** | Structured logging: levels, key-value fields, text/JSON output, stderr/stdout/file sinks |
| **nmail** | MIME email compose + parse: attachments, HTML+text alternatives, inline images. ~email (pairs nsmtp) ... |
| **nmdns** | mDNS / DNS-SD service discovery and announcement (zeroconf). ~zeroconf ... |
| **nmqtt** | MQTT 3.1.1/5 client: QoS 0-2, TLS, reconnect, wills. ~paho-mqtt (key for IoT / edge fleets) |
| **nopenapi** | OpenAPI 3 spec generation (from ahiru routes) + typed client stub generation. ~fastapi openapi, openapi-gen ... |
| **notp** | TOTP / HOTP two-factor codes, provisioning URIs. ~pyotp ... |
| **nreq** | Ergonomic HTTP client: sessions, cookie jar, retries, redirects, connection pooling, multipart upload, streaming download, proxies. ~requests, httpx (ergonomic layer over net, like requests over urllib) |
| **nrpc** | JSON-RPC 2.0 client/server over stdio, TCP, HTTP. ~jsonrpcserver ... |
| **nscrape** | Polite scraping: robots.txt, rate limits, retries, sitemap crawl, article/readability extraction. ~scrapy, trafilatura, newspaper |
| **nsmtp** | Ergonomic SMTP email sending with object-based config |
| **nssh** | SSH client: exec, interactive shell, SFTP, port forwarding, agent + key auth. ~paramiko, fabric ... |
| **ntrace** | Distributed tracing spans, W3C traceparent, events, JSON export |
| **nwebhook** | Webhook send/receive: HMAC signing + verification, timestamps, replay defense. ~svix, standard-webhooks ... |
| **nws** | Ergonomic WebSocket client wrapper over net (shared handles) |

## Databases & storage

| Library | Description |
|---------|-------------|
| **ncache** | In-memory LRU and TTL caches with hit/miss statistics |
| **ndocstore** | Embedded JSON document store with queries + secondary indexes. ~tinydb |
| **nfts** | Embedded full-text search: inverted index, BM25, phrase/prefix, facets (tantivy-class). ~whoosh (pairs with nvec for hybrid keyword+vector RAG) |
| **nkv** | Embedded ordered key-value store: ACID, prefix scans, snapshots (LMDB/redb-class). ~lmdb, shelve, diskcache |
| **nmigrate** | Schema diff from nmodel struct definitions to SQL migrations (sqlite/postgres) |
| **nmodel** | Prisma-style ORM over nsqlite and npg: schema DSL, auto-migrations, CRUD |
| **nmongo** | Fast MongoDB: CRUD, aggregation, indexes, transactions, GridFS, change streams, async |
| **nmysql** | MySQL / MariaDB client: pools, prepared statements, transactions, async. ~pymysql, mysqlclient (completes the big-4 next to npg/nsqlite/nmongo) |
| **npg** | Fast PostgreSQL: pools, migrations, prepared statements, transactions, async |
| **nredis** | Redis client: get/set/del/incr/expire, hash ops, mget/mset, raw RESP commands |
| **nsearch** | Hosted search-engine clients: Elasticsearch/OpenSearch, Meilisearch, Typesense. ~elasticsearch, meilisearch |
| **nsqlite** | Fast SQLite: schema, migrations, prepared statements, transactions, async |
| **nsupa** | Supabase client: PostgREST query builder, GoTrue auth, Storage REST — zero-dep over HTTP |
| **nvec** | Vector database: in-memory cosine similarity index (NSW/HNSW-lite) with optional Qdrant REST backend |

## Data & analytics

| Library | Description |
|---------|-------------|
| **ncl** | Niao Column Library — fast pandas/numpy-style columnar data and math |
| **nframe** | Columnar DataFrame / Series: groupby, join, reshape, rolling, CSV/JSON IO (pandas/polars subset) |
| **nparquet** | Parquet + Arrow IPC read/write, nframe interop for data interchange. ~pyarrow (beside ncolumnar's NCOL1) ... |
| **nplot** | SVG-first plotting for EDA and model diagnostics (matplotlib/seaborn subset) |
| **nsoa** | Columnar struct-of-arrays tables with typed columns |
| **nstats** | Statistics: distributions, hypothesis tests, correlation, OLS (scipy.stats + statsmodels core) |
| **nts** | Time series: ACF/PACF, decomposition, ARIMA/SARIMA, Holt-Winters (statsmodels.tsa core) |
| **nvis** | Niao visualization — line, histogram, scatter, heatmap, bar charts |

## Math & numerics

| Library | Description |
|---------|-------------|
| **ndecimal** | Arbitrary-precision decimals + rationals, money-safe rounding modes. ~decimal, fractions (pairs bignum) ... |
| **ndsp** | Signal processing: FIR/IIR filters, windows, convolution, resampling, spectrograms. ~scipy.signal (pairs nnum FFT + naudio) |
| **nfin** | Financial math: TVM, NPV/IRR, amortization, returns, common technical indicators. ~numpy-financial, TA-Lib |
| **nmath** | Scalar math and statistics: trig, logs, rounding, combinatorics, mean/median/stdev/percentile |
| **nnum** | Numeric foundation: n-dim arrays, linear algebra, FFT (numpy/scipy.linalg/scipy.fft subset) |
| **noptim** | Optimization: minimize, root finding, least squares, LP (scipy.optimize subset) |

## Machine learning

| Library | Description |
|---------|-------------|
| **nboost** | Histogram gradient-boosted decision trees (XGBoost / LightGBM subset) |
| **ndataset** | Dataset loading: splits, shuffling, streaming batches, common formats. ~datasets, torch dataloader |
| **neval** | Model evaluation metrics, dataset runner, and latency benchmarking |
| **nlearn** | Classical ML: estimators, preprocessing, Pipeline (scikit-learn subset) |
| **nml** | Niao Machine Learning — fast tensors, GPU training, classic ML |
| **nnlp** | Classical NLP: normalization, TF-IDF, n-grams, word2vec, similarity, baselines |
| **nsketch** | Probabilistic sketches: Bloom filter, HyperLogLog-lite, Count-Min Sketch |
| **nspeech** | Speech-to-text via whisper.cpp: files + mic, timestamps, VAD. ~openai-whisper, speechrecognition (edge-friendly, fits low-end device goal) |
| **ntts** | Text-to-speech via piper/espeak: synth to WAV, voice selection. ~pyttsx3 |
| **ntune** | Hyperparameter search: grid, random, successive halving over nlearn/neval. ~optuna |
| **nvision** | Computer vision: image IO, transforms, classical CV, dataset loaders (torchvision/OpenCV/Pillow subset) |

## AI & LLM

| Library | Description |
|---------|-------------|
| **nctx** | Token estimates, trim strategies, message budgets, and conversation stats |
| **nembed** | Content-hash embedding cache with local deterministic SHA-256 embedder |
| **nguard** | PII scan/redact and denylist middleware hooks for AI safety |
| **nhub** | Model/dataset downloads: resume, cache dir, checksums, HF Hub + direct URLs. ~huggingface-hub (feeds nllm / nonnx) |
| **nllm** | Fast GGUF LLM inference: complete, chat, stream, tokenize via llama.cpp + Candle |
| **nmem** | Script long-term memory with KV, TTL, tags, and export/import |
| **nprompt** | Interactive TTY prompts on stdin/stdout with pipe fallback |
| **nprovider** | Provider profiles, model aliases, failover chains, and LLM pricing table |
| **nrag** | Fast vector RAG: batch embeddings, parallel cosine search, index build/save |
| **nschema** | JSON schema from example, validate/coerce/parse, LLM prompt and tool specs |
| **ntemplate** | Versioned prompt templates with variable injection and token estimation |
| **ntok** | Byte-level BPE tokenizer with encode/decode/count, chunk, and context fit |

## Cloud & integrations

| Library | Description |
|---------|-------------|
| **naws** | AWS helper — SigV4-signed S3, DynamoDB, Lambda, and SSM Parameter Store |
| **nazure** | Azure helper: Blob Storage (put/get/delete/list), Table Storage (insert/query/delete), Azure Functions HTTP trigger. SharedKey + SAS + Bearer auth. |
| **nblob** | Unified object-store VFS: local dir, S3, Azure Blob, GCS behind one open/read/write/list API. ~fsspec, smart_open (over naws / nazure / ngcp) |
| **nbudget** | Cooperative resource and cost budgets: cpu/ram/gpu/usd/tokens limits, charge, check, remain |
| **ncost** | Preflight USD estimates for LLM tokens, S3 storage, and Lambda compute |
| **ngcp** | Google Cloud helper: GCS, Pub/Sub, Firestore REST, Cloud Functions. ~google-cloud-* (peers naws/nazure) |
| **nquota** | Token-bucket rate limiting with refill based on wall-clock elapsed time |

## System & runtime

| Library | Description |
|---------|-------------|
| **args** | CLI argument parsing — flags, options, positionals, subcommands |
| **nargs** | CLI argument parsing: flags, typed options, positionals, --key=value, generated help |
| **nbatch** | Adaptive batch sizing: suggest from VRAM/RAM, fit steps, clamp/scale/halve |
| **ncap** | Cooperative capability sandbox (grant/revoke/require) |
| **nconfig** | Layered config defaults, file, env, args with typed schema validation |
| **ncpu** | CPU detection, usage, temperature, cooperative core limits, and thread recommendations |
| **ncrash** | Structured JSON crash reports, wrap(fn), and fingerprints |
| **ncron** | Standard 5-field cron parse, validate, match, and next-run computation |
| **ndevice** | Unified hardware detection, thermal profiles, safety guard, pacing, and device selection |
| **nenv** | Environment variables, .env loading, typed accessors, validation, and stores |
| **nevent** | In-process event emitter / pub-sub with typed topics and wildcards. ~blinker, pyee ... |
| **nfallback** | Graceful degradation: value chains and named circuit breakers |
| **nfs** | High-level file ops: copy/move trees, atomic write, temp files/dirs, disk usage, trash. ~shutil, tempfile, send2trash (above io handles / nos basics) |
| **ngpu** | GPU detection, VRAM and utilization readings, cooperative budgets, and thermal gating |
| **nhotreload** | File watch and per-function body diff via niao_parser |
| **nkeyring** | OS credential stores: macOS Keychain, Secret Service, Windows DPAPI. ~keyring (subset) |
| **nnpu** | Best-effort NPU detection and advisory budget for neural accelerator workloads |
| **nos** | OS interface: process, platform constants, lightweight filesystem |
| **npace** | Adaptive loop pacing by level, temperature, and load |
| **nram** | System and process memory readings, cooperative RAM budgets, and pressure gating |
| **nretry** | Retry with exponential backoff, jitter, deadlines, retry-on predicates. ~tenacity, backoff (complements nfallback circuit breakers)... |
| **nsignal** | OS signal handlers, graceful-shutdown patterns, SIGTERM/SIGINT hooks. ~signal (stdlib subset) |
| **nwatch** | Reactive poll watchers for file mtimes and in-memory values |
| **nworkspace** | Workspace manifest, member graph, topo order, and run |

## Strings, validation & utilities

| Library | Description |
|---------|-------------|
| **ncolor** | Terminal styling: named colors, 256/truecolor, bold/underline, strip, NO_COLOR aware |
| **ndiff** | Deep structural equality and diff for values, arrays, and objects |
| **nerrgen** | E-code spec file parser and rust/niao/markdown artifact generator |
| **nexplain** | Actionable error enrichment: pattern hints, custom rules, pretty format |
| **nfmt** | Formatting: {} templates, thousands separators, hex/oct/bin, humanized bytes/durations/counts |
| **nfsm** | Finite state machines + statecharts: states, guards, transitions, hooks. ~transitions, python-statemachine ... |
| **nfunc** | Function toolkit: memoize/LRU, partial, compose, curry, once, debounce, throttle. ~functools, toolz ... |
| **nid** | ID generation: ULID, UUIDv7, nanoid, snowflake, hashids. ~uuid6, ulid-py (extends codec's UUID) ... |
| **npass** | Password hashing: argon2id, bcrypt, scrypt + strength policy checks. ~passlib, argon2-cffi, bcrypt ... |
| **nsanitize** | Allowlist HTML sanitizer for user content (XSS-safe), URL scheme policy. ~bleach, nh3 ... |
| **nsemver** | SemVer 2.0 parse, compare, range matching, and version increment |
| **nshape** | Value shape description, rank/dims, match, and simple schema checks |
| **nstr** | String toolkit: case conversions, trim/pad/wrap, split/join, search, slugify, edit distance |
| **ntextdiff** | Line/word text diff, unified patches, 3-way merge. ~difflib, diff-match-patch (beside ndiff structural) ... |
| **nunicode** | Unicode correctness: NFC/NFD normalization, grapheme clusters, categories, display width, casefold. ~unicodedata, grapheme (below nstr string ops) |
| **nvalid** | Data validation: schema rules, email/url/uuid/ipv4 checks, pattern matching |
| **nwhy** | Value lineage and provenance tracking with explain and graph |

## Developer tools & testing

| Library | Description |
|---------|-------------|
| **nbench** | Micro-benchmark harness with warmup, percentiles, and compare |
| **ncassette** | VCR-style request/response cassette for record, replay, and passthrough |
| **ncontract** | Design-by-contract: require/ensure/check, assert_type, object invariants |
| **ndebug** | Checkpoint time-travel over values with deep diff |
| **ndoc** | Doc-comment doctest extraction and execution (// >>> and // =>) |
| **nfuzz** | Deterministic property/fuzz helpers (xorshift64 seed, int/float/string/bytes/cases) |
| **nlint** | AST-as-data linting via niao_parser with data-driven rules and nlint_check diagnostics |
| **nproc** | Child processes beyond nshell: process pools, pipes, IPC channels, shared memory. ~multiprocessing |
| **nprofile** | Micro timing spans, named sample recording, and latency stats (p50/p95) |
| **nreflect** | Runtime introspection: function arity/params, doc strings, module listing, source location. ~inspect ... |
| **nrepl** | Subprocess expression evaluation REPL sessions |
| **nreplay** | Deterministic event record/replay sessions with save/load |
| **nscaffold** | CRUD route, nmodel schema, SQL migration, and ntest generation from struct spec |
| **nshell** | Subprocess execution with captured output, timeouts, and PATH lookup |
| **ntest** | Testing: case registration, runner with summaries, assert_eq/near/contains/error |

## Applications & frameworks

| Library | Description |
|---------|-------------|
| **ahiru** | ahiru-server 0.3.0: state, custom middleware, groups, cache, jobs, metrics, CLI toolkit |
| **nagent** | Lightweight multi-agent orchestration scaffolding (no LLM) |

## Other

| Library | Description |
|---------|-------------|
| **nbrowser** | Headless browser automation via CDP/WebDriver: navigate, click, fill, screenshot, PDF. ~playwright, selenium |
| **ncompress** | Modern compression: zstd, lz4, brotli, xz -- block and stream APIs. ~zstandard, lz4, brotli (extends archive's gzip/deflate) ... |
| **nexpr** | Safe sandboxed expression evaluator for user formulas and config logic. ~simpleeval, asteval... |
| **nfeed** | RSS / Atom feed parse + generate. ~feedparser ... |
| **nflock** | Advisory file locks, lockfiles, PID files, timeouts. ~filelock, fcntl |
| **ngraph** | Graph algorithms: shortest paths, centrality, communities, flows, toposort, layouts. ~networkx (extends dsa's basic graph) |
| **noauth** | OAuth2 + OIDC client flows: auth code, PKCE, client credentials, token refresh. ~authlib, oauthlib ... |
| **nonnx** | ONNX model loading + CPU inference for small models. ~onnxruntime |
| **nsign** | Signed + expiring tokens, cookies, URLs (tamper-proof values). ~itsdangerous ... |
| **nunits** | Physical units + quantity arithmetic, conversion, dimensional checks. ~pint |
| **nwhen** | Natural-language + fuzzy date parsing ("next friday 5pm", "in 2 weeks"). ~dateparser, dateutil (extends time) ... |
| **nxml** | XML DOM + streaming (SAX-style) parser, namespaces, XPath subset, pretty-print. ~xml.etree, lxml ... |

## Alphabetical index

| Library | Description |
|---------|-------------|
| **ahiru** | ahiru-server 0.3.0: state, custom middleware, groups, cache, jobs, metrics, CLI toolkit |
| **archive** | Gzip and deflate compression helpers |
| **args** | CLI argument parsing — flags, options, positionals, subcommands |
| **bignum** | Arbitrary-precision integer arithmetic |
| **codec** | Base64, hex, UUID, and dotenv helpers |
| **collections** | Insertion-ordered maps/sets (IndexMap/IndexSet) and fast hashing |
| **core** | Core builtins: print, len, type, assert, errors, timing, arrays |
| **crypto** | SHA-256/512 and HMAC helpers |
| **dsa** | Data structures and algorithms: list, stack, queue, heap, map, graph, sort |
| **http** | HTTP types: Method, StatusCode, HeaderMap, Uri |
| **io** | File I/O, paths, streaming handles, async tasks, raw socket options (niao_io) |
| **json** | JSON parse, stringify, and object utilities |
| **log** | Structured logging, level filtering, and spans |
| **nagent** | Lightweight multi-agent orchestration scaffolding (no LLM) |
| **narena** | Pooled packed-buffer reuse arena for GC-pressure relief |
| **nargs** | CLI argument parsing: flags, typed options, positionals, --key=value, generated help |
| **nasync** | Structured async ergonomics over io tasks: gather/race, timeouts, cancellation, async channels. ~asyncio, trio subset |
| **nauth** | Web auth kit: sessions, login/logout, password reset, RBAC roles, CSRF tokens. ~flask-login, django auth (built on npass + nsign) |
| **naws** | AWS helper — SigV4-signed S3, DynamoDB, Lambda, and SSM Parameter Store |
| **nazure** | Azure helper: Blob Storage (put/get/delete/list), Table Storage (insert/query/delete), Azure Functions HTTP trigger. SharedKey + SAS + Bearer auth. |
| **nbatch** | Adaptive batch sizing: suggest from VRAM/RAM, fit steps, clamp/scale/halve |
| **nbench** | Micro-benchmark harness with warmup, percentiles, and compare |
| **nbinary** | Binary data: struct pack/unpack, endianness, bit fields, varints, CRC32/64. ~struct, bitstring ... |
| **nblob** | Unified object-store VFS: local dir, S3, Azure Blob, GCS behind one open/read/write/list API. ~fsspec, smart_open (over naws / nazure / ngcp) |
| **nboost** | Histogram gradient-boosted decision trees (XGBoost / LightGBM subset) |
| **nbrowser** | Headless browser automation via CDP/WebDriver: navigate, click, fill, screenshot, PDF. ~playwright, selenium |
| **nbudget** | Cooperative resource and cost budgets: cpu/ram/gpu/usd/tokens limits, charge, check, remain |
| **ncache** | In-memory LRU and TTL caches with hit/miss statistics |
| **ncal** | Calendar math: business days, holiday tables, week numbers, month grids. ~calendar, holidays, workalendar ... |
| **ncanon** | Canonical JSON-like encoding, FNV-1a hash, structural equal, fingerprint |
| **ncap** | Cooperative capability sandbox (grant/revoke/require) |
| **ncassette** | VCR-style request/response cassette for record, replay, and passthrough |
| **ncbor** | CBOR encode/decode (IoT / COSE friendly). ~cbor2 ... |
| **ncl** | Niao Column Library — fast pandas/numpy-style columnar data and math |
| **ncolor** | Terminal styling: named colors, 256/truecolor, bold/underline, strip, NO_COLOR aware |
| **ncolumnar** | Column-major binary codec for tables (magic NCOL1) |
| **ncompress** | Modern compression: zstd, lz4, brotli, xz -- block and stream APIs. ~zstandard, lz4, brotli (extends archive's gzip/deflate) ... |
| **nconfig** | Layered config defaults, file, env, args with typed schema validation |
| **ncontract** | Design-by-contract: require/ensure/check, assert_type, object invariants |
| **ncost** | Preflight USD estimates for LLM tokens, S3 storage, and Lambda compute |
| **ncpu** | CPU detection, usage, temperature, cooperative core limits, and thread recommendations |
| **ncrash** | Structured JSON crash reports, wrap(fn), and fingerprints |
| **ncron** | Standard 5-field cron parse, validate, match, and next-run computation |
| **ncrypt** | Modern crypto: AES-GCM, ChaCha20-Poly1305, RSA, Ed25519/X25519, HKDF/PBKDF2, X.509 parse, CSPRNG, constant-time compare. ~cryptography, pynacl, secrets (extends crypto's SHA/HMAC) ... |
| **ncsv** | Lightweight CSV parse/stringify and file read/write |
| **nctx** | Token estimates, trim strategies, message budgets, and conversation stats |
| **ndataset** | Dataset loading: splits, shuffling, streaming batches, common formats. ~datasets, torch dataloader |
| **ndebug** | Checkpoint time-travel over values with deep diff |
| **ndecimal** | Arbitrary-precision decimals + rationals, money-safe rounding modes. ~decimal, fractions (pairs bignum) ... |
| **ndevice** | Unified hardware detection, thermal profiles, safety guard, pacing, and device selection |
| **ndiff** | Deep structural equality and diff for values, arrays, and objects |
| **ndoc** | Doc-comment doctest extraction and execution (// >>> and // =>) |
| **ndocstore** | Embedded JSON document store with queries + secondary indexes. ~tinydb |
| **ndsp** | Signal processing: FIR/IIR filters, windows, convolution, resampling, spectrograms. ~scipy.signal (pairs nnum FFT + naudio) |
| **nembed** | Content-hash embedding cache with local deterministic SHA-256 embedder |
| **nencoding** | Charset detection + transcoding: UTF-16, Shift-JIS, GBK, Latin-1, BOM handling. ~codecs, charset-normalizer... |
| **nenv** | Environment variables, .env loading, typed accessors, validation, and stores |
| **nerrgen** | E-code spec file parser and rust/niao/markdown artifact generator |
| **net** | Networking: HTTP, TCP/UDP, DNS, TLS, WebSocket, SMTP, FTP |
| **net_clients** | SMTP and FTP client helpers |
| **neval** | Model evaluation metrics, dataset runner, and latency benchmarking |
| **nevent** | In-process event emitter / pub-sub with typed topics and wildcards. ~blinker, pyee ... |
| **nexplain** | Actionable error enrichment: pattern hints, custom rules, pretty format |
| **nexpr** | Safe sandboxed expression evaluator for user formulas and config logic. ~simpleeval, asteval... |
| **nfallback** | Graceful degradation: value chains and named circuit breakers |
| **nfeed** | RSS / Atom feed parse + generate. ~feedparser ... |
| **nfin** | Financial math: TVM, NPV/IRR, amortization, returns, common technical indicators. ~numpy-financial, TA-Lib |
| **nflock** | Advisory file locks, lockfiles, PID files, timeouts. ~filelock, fcntl |
| **nfmt** | Formatting: {} templates, thousands separators, hex/oct/bin, humanized bytes/durations/counts |
| **nframe** | Columnar DataFrame / Series: groupby, join, reshape, rolling, CSV/JSON IO (pandas/polars subset) |
| **nfs** | High-level file ops: copy/move trees, atomic write, temp files/dirs, disk usage, trash. ~shutil, tempfile, send2trash (above io handles / nos basics) |
| **nfsm** | Finite state machines + statecharts: states, guards, transitions, hooks. ~transitions, python-statemachine ... |
| **nfts** | Embedded full-text search: inverted index, BM25, phrase/prefix, facets (tantivy-class). ~whoosh (pairs with nvec for hybrid keyword+vector RAG) |
| **nfunc** | Function toolkit: memoize/LRU, partial, compose, curry, once, debounce, throttle. ~functools, toolz ... |
| **nfuzz** | Deterministic property/fuzz helpers (xorshift64 seed, int/float/string/bytes/cases) |
| **ngcp** | Google Cloud helper: GCS, Pub/Sub, Firestore REST, Cloud Functions. ~google-cloud-* (peers naws/nazure) |
| **ngeo** | Geospatial: haversine, GeoJSON, points/polygons, bounding boxes, tile math. ~shapely, geopy, geojson |
| **nglob** | Glob patterns, ** recursion, gitignore-style matching, walk with filters. ~glob, fnmatch, pathspec... |
| **ngpu** | GPU detection, VRAM and utilization readings, cooperative budgets, and thermal gating |
| **ngraph** | Graph algorithms: shortest paths, centrality, communities, flows, toposort, layouts. ~networkx (extends dsa's basic graph) |
| **ngraphql** | GraphQL client (queries, variables, fragments) + schema/server helpers. ~gql, graphene, strawberry ... |
| **nguard** | PII scan/redact and denylist middleware hooks for AI safety |
| **nhdf5** | HDF5 scientific dataset read/write, groups, attrs. ~h5py ... |
| **nhotreload** | File watch and per-function body diff via niao_parser |
| **nhtml** | Forgiving HTML5 parser, CSS selectors, tree walking, text extraction, escape/unescape. ~BeautifulSoup4 ... |
| **nhub** | Model/dataset downloads: resume, cache dir, checksums, HF Hub + direct URLs. ~huggingface-hub (feeds nllm / nonnx) |
| **nical** | iCalendar / vCard parse + generate, recurrence rules (RRULE). ~icalendar, vobject ... |
| **nid** | ID generation: ULID, UUIDv7, nanoid, snowflake, hashids. ~uuid6, ulid-py (extends codec's UUID) ... |
| **nimap** | IMAP4 + POP3 mailbox retrieval: search, flags, folders, IDLE push. ~imaplib, imapclient ... |
| **nipaddr** | IPv4/IPv6 addresses, CIDR networks, ranges, subnet math, membership checks. ~ipaddress... |
| **niter** | Iterator & combinatorics toolkit: product, permutations, combinations, groupby, windows, chunked, flatten, zip_longest. ~itertools, more-itertools (general values; complements nlazy packed pipelines) ... |
| **njpath** | JSONPath / JMESPath-style queries, JSON Pointer and JSON Patch over values. ~jmespath, jsonpath-ng, glom ... |
| **njwt** | JWT / JWS sign + verify (HS/RS/ES/EdDSA), claims and expiry validation, JWKS fetch. ~pyjwt, python-jose ... |
| **nkeyring** | OS credential stores: macOS Keychain, Secret Service, Windows DPAPI. ~keyring (subset) |
| **nkv** | Embedded ordered key-value store: ACID, prefix scans, snapshots (LMDB/redb-class). ~lmdb, shelve, diskcache |
| **nlazy** | Fused lazy map/filter/take pipelines over packed arrays |
| **nlearn** | Classical ML: estimators, preprocessing, Pipeline (scikit-learn subset) |
| **nlint** | AST-as-data linting via niao_parser with data-driven rules and nlint_check diagnostics |
| **nllm** | Fast GGUF LLM inference: complete, chat, stream, tokenize via llama.cpp + Candle |
| **nlog** | Structured logging: levels, key-value fields, text/JSON output, stderr/stdout/file sinks |
| **nmail** | MIME email compose + parse: attachments, HTML+text alternatives, inline images. ~email (pairs nsmtp) ... |
| **nmarkdown** | Lightweight Markdown to HTML, plain-text strip, and heading extraction |
| **nmath** | Scalar math and statistics: trig, logs, rounding, combinatorics, mean/median/stdev/percentile |
| **nmdns** | mDNS / DNS-SD service discovery and announcement (zeroconf). ~zeroconf ... |
| **nmem** | Script long-term memory with KV, TTL, tags, and export/import |
| **nmigrate** | Schema diff from nmodel struct definitions to SQL migrations (sqlite/postgres) |
| **nmime** | File-type detection by magic bytes, extension<->MIME maps. ~python-magic, filetype ... |
| **nml** | Niao Machine Learning — fast tensors, GPU training, classic ML |
| **nmmap** | Memory-mapped files via memmap2, lazy line index, byte search |
| **nmodel** | Prisma-style ORM over nsqlite and npg: schema DSL, auto-migrations, CRUD |
| **nmongo** | Fast MongoDB: CRUD, aggregation, indexes, transactions, GridFS, change streams, async |
| **nmqtt** | MQTT 3.1.1/5 client: QoS 0-2, TLS, reconnect, wills. ~paho-mqtt (key for IoT / edge fleets) |
| **nmsgpack** | MessagePack encode/decode, streaming. ~msgpack... |
| **nmysql** | MySQL / MariaDB client: pools, prepared statements, transactions, async. ~pymysql, mysqlclient (completes the big-4 next to npg/nsqlite/nmongo) |
| **nnlp** | Classical NLP: normalization, TF-IDF, n-grams, word2vec, similarity, baselines |
| **nnpu** | Best-effort NPU detection and advisory budget for neural accelerator workloads |
| **nnum** | Numeric foundation: n-dim arrays, linear algebra, FFT (numpy/scipy.linalg/scipy.fft subset) |
| **noauth** | OAuth2 + OIDC client flows: auth code, PKCE, client credentials, token refresh. ~authlib, oauthlib ... |
| **nonnx** | ONNX model loading + CPU inference for small models. ~onnxruntime |
| **nopenapi** | OpenAPI 3 spec generation (from ahiru routes) + typed client stub generation. ~fastapi openapi, openapi-gen ... |
| **noptim** | Optimization: minimize, root finding, least squares, LP (scipy.optimize subset) |
| **nos** | OS interface: process, platform constants, lightweight filesystem |
| **notp** | TOTP / HOTP two-factor codes, provisioning URIs. ~pyotp ... |
| **npace** | Adaptive loop pacing by level, temperature, and load |
| **npar** | Explicit rayon parallel ops on packed arrays with set_threads |
| **nparquet** | Parquet + Arrow IPC read/write, nframe interop for data interchange. ~pyarrow (beside ncolumnar's NCOL1) ... |
| **npass** | Password hashing: argon2id, bcrypt, scrypt + strength policy checks. ~passlib, argon2-cffi, bcrypt ... |
| **npdf** | PDF create (text, images, tables) + extract text and pages, merge/split. ~reportlab, pypdf ... |
| **npersist** | im-rc persistent Vector/HashMap with structural sharing |
| **npg** | Fast PostgreSQL: pools, migrations, prepared statements, transactions, async |
| **npipe** | Typed step pipelines over built-in ops (id, len, type, keys, not_nil, str, abs) |
| **nplot** | SVG-first plotting for EDA and model diagnostics (matplotlib/seaborn subset) |
| **nproc** | Child processes beyond nshell: process pools, pipes, IPC channels, shared memory. ~multiprocessing |
| **nprofile** | Micro timing spans, named sample recording, and latency stats (p50/p95) |
| **nprompt** | Interactive TTY prompts on stdin/stdout with pipe fallback |
| **nproto** | Protocol Buffers codec + codegen from .proto files. ~protobuf ... |
| **nprovider** | Provider profiles, model aliases, failover chains, and LLM pricing table |
| **nquota** | Token-bucket rate limiting with refill based on wall-clock elapsed time |
| **nrag** | Fast vector RAG: batch embeddings, parallel cosine search, index build/save |
| **nram** | System and process memory readings, cooperative RAM budgets, and pressure gating |
| **nrand** | Fast random numbers (xoshiro256**): ints, floats, strings, choice/shuffle/sample, distributions |
| **nredis** | Redis client: get/set/del/incr/expire, hash ops, mget/mset, raw RESP commands |
| **nreflect** | Runtime introspection: function arity/params, doc strings, module listing, source location. ~inspect ... |
| **nrepl** | Subprocess expression evaluation REPL sessions |
| **nreplay** | Deterministic event record/replay sessions with save/load |
| **nreq** | Ergonomic HTTP client: sessions, cookie jar, retries, redirects, connection pooling, multipart upload, streaming download, proxies. ~requests, httpx (ergonomic layer over net, like requests over urllib) |
| **nretry** | Retry with exponential backoff, jitter, deadlines, retry-on predicates. ~tenacity, backoff (complements nfallback circuit breakers)... |
| **nrpc** | JSON-RPC 2.0 client/server over stdio, TCP, HTTP. ~jsonrpcserver ... |
| **nsanitize** | Allowlist HTML sanitizer for user content (XSS-safe), URL scheme policy. ~bleach, nh3 ... |
| **nscaffold** | CRUD route, nmodel schema, SQL migration, and ntest generation from struct spec |
| **nschema** | JSON schema from example, validate/coerce/parse, LLM prompt and tool specs |
| **nscrape** | Polite scraping: robots.txt, rate limits, retries, sitemap crawl, article/readability extraction. ~scrapy, trafilatura, newspaper |
| **nsearch** | Hosted search-engine clients: Elasticsearch/OpenSearch, Meilisearch, Typesense. ~elasticsearch, meilisearch |
| **nsemver** | SemVer 2.0 parse, compare, range matching, and version increment |
| **nshape** | Value shape description, rank/dims, match, and simple schema checks |
| **nshell** | Subprocess execution with captured output, timeouts, and PATH lookup |
| **nsign** | Signed + expiring tokens, cookies, URLs (tamper-proof values). ~itsdangerous ... |
| **nsignal** | OS signal handlers, graceful-shutdown patterns, SIGTERM/SIGINT hooks. ~signal (stdlib subset) |
| **nsimd** | Unrolled autovectorized f64/i64 kernels on packed FloatArray/IntArray |
| **nsketch** | Probabilistic sketches: Bloom filter, HyperLogLog-lite, Count-Min Sketch |
| **nsmtp** | Ergonomic SMTP email sending with object-based config |
| **nsnap** | Fast binary value snapshots with fingerprints and staleness checks (NSNP1) |
| **nsoa** | Columnar struct-of-arrays tables with typed columns |
| **nsorted** | Sorted list / dict / set with bisect insert, range queries, nearest lookup. ~sortedcontainers, bisect ... |
| **nspeech** | Speech-to-text via whisper.cpp: files + mic, timestamps, VAD. ~openai-whisper, speechrecognition (edge-friendly, fits low-end device goal) |
| **nsqlite** | Fast SQLite: schema, migrations, prepared statements, transactions, async |
| **nssh** | SSH client: exec, interactive shell, SFTP, port forwarding, agent + key auth. ~paramiko, fabric ... |
| **nstats** | Statistics: distributions, hypothesis tests, correlation, OLS (scipy.stats + statsmodels core) |
| **nstr** | String toolkit: case conversions, trim/pad/wrap, split/join, search, slugify, edit distance |
| **nsupa** | Supabase client: PostgREST query builder, GoTrue auth, Storage REST — zero-dep over HTTP |
| **ntar** | tar archives read/write incl. .tar.gz / .tar.zst. ~tarfile ... |
| **ntemplate** | Versioned prompt templates with variable injection and token estimation |
| **ntest** | Testing: case registration, runner with summaries, assert_eq/near/contains/error |
| **ntextdiff** | Line/word text diff, unified patches, 3-way merge. ~difflib, diff-match-patch (beside ndiff structural) ... |
| **ntok** | Byte-level BPE tokenizer with encode/decode/count, chunk, and context fit |
| **ntoml** | TOML parse and stringify for configuration files |
| **ntrace** | Distributed tracing spans, W3C traceparent, events, JSON export |
| **nts** | Time series: ACF/PACF, decomposition, ARIMA/SARIMA, Holt-Winters (statsmodels.tsa core) |
| **ntts** | Text-to-speech via piper/espeak: synth to WAV, voice selection. ~pyttsx3 |
| **ntune** | Hyperparameter search: grid, random, successive halving over nlearn/neval. ~optuna |
| **nunicode** | Unicode correctness: NFC/NFD normalization, grapheme clusters, categories, display width, casefold. ~unicodedata, grapheme (below nstr string ops) |
| **nunits** | Physical units + quantity arithmetic, conversion, dimensional checks. ~pint |
| **nurl** | URL parse, build, join, query helpers, and percent encoding |
| **nvalid** | Data validation: schema rules, email/url/uuid/ipv4 checks, pattern matching |
| **nvec** | Vector database: in-memory cosine similarity index (NSW/HNSW-lite) with optional Qdrant REST backend |
| **nview** | Jinja-style templating: inheritance, blocks, filters, autoescape, partials -- for HTML/text output. ~jinja2 (distinct from ntemplate's LLM prompt templates) |
| **nvis** | Niao visualization — line, histogram, scatter, heatmap, bar charts |
| **nvision** | Computer vision: image IO, transforms, classical CV, dataset loaders (torchvision/OpenCV/Pillow subset) |
| **nwatch** | Reactive poll watchers for file mtimes and in-memory values |
| **nwebhook** | Webhook send/receive: HMAC signing + verification, timestamps, replay defense. ~svix, standard-webhooks ... |
| **nwhen** | Natural-language + fuzzy date parsing ("next friday 5pm", "in 2 weeks"). ~dateparser, dateutil (extends time) ... |
| **nwhy** | Value lineage and provenance tracking with explain and graph |
| **nworkspace** | Workspace manifest, member graph, topo order, and run |
| **nws** | Ergonomic WebSocket client wrapper over net (shared handles) |
| **nxlsx** | Excel .xlsx read/write: sheets, styles, formulas, streaming rows. ~openpyxl, xlsxwriter ... |
| **nxml** | XML DOM + streaming (SAX-style) parser, namespaces, XPath subset, pretty-print. ~xml.etree, lxml ... |
| **nyaml** | YAML 1.2 parse + emit, safe-by-default, anchors, multi-doc. ~PyYAML, ruamel.yaml... |
| **nzip** | ZIP archives: read/write, streaming, per-entry compression, encryption. ~zipfile ... |
| **parallel** | Threading, mutexes, channels, worker pools, and cooperative poll |
| **rand** | Seeded PRNG, uniform ranges, shuffle, and choose |
| **re** | Regular expressions: match, find, replace, split |
| **time** | Wall clock, formatting, parsing, time zones, and date arithmetic |
