//! Canonical error code registry for the Niao toolchain.
//!
//! Codes are grouped by category:
//! - `E0001`–`E0099` — lexer
//! - `E0100`–`E0199` — parser
//! - `E0200`–`E0299` — compiler / IR
//! - `E1000`–`E1099` — builtins
//! - `E1100`–`E1199` — DSA builtins
//! - `E2000`–`E2099` — runtime semantics
//! - `W0001`–`W0099` — linter warnings

/// Unexpected character during lexing.
pub const E0001_UNEXPECTED_CHAR: u32 = 1;
/// Unterminated string literal.
pub const E0002_UNTERMINATED_STRING: u32 = 2;

/// Unexpected token during parsing.
pub const E0100_UNEXPECTED_TOKEN: u32 = 100;
/// Unexpected end of file during parsing.
pub const E0101_UNEXPECTED_EOF: u32 = 101;

/// Unsupported construct during IR lowering.
pub const E0200_UNSUPPORTED: u32 = 200;
/// Unknown function at compile time.
pub const E0201_UNKNOWN_FUNCTION: u32 = 201;

/// Builtin called with wrong arity.
pub const E1001_BUILTIN_ARITY: u32 = 1001;
/// `type()` builtin arity error.
pub const E1002_TYPE_BUILTIN: u32 = 1002;
/// `assert()` builtin arity error.
pub const E1003_ASSERT_ARITY: u32 = 1003;
/// Wrong function argument count.
pub const E1004_ARG_COUNT: u32 = 1004;
/// Invalid control flow (break/continue outside loop).
pub const E1005_CONTROL_FLOW: u32 = 1005;
/// Index or field access error.
pub const E1006_INDEX_FIELD: u32 = 1006;
/// Array allocation arity error.
pub const E1007_ARRAY_ALLOC: u32 = 1007;
/// Index out of bounds.
pub const E1008_INDEX_BOUNDS: u32 = 1008;
/// Unknown struct or sort arity.
pub const E1009_STRUCT_SORT: u32 = 1009;
/// Unknown struct field.
pub const E1010_STRUCT_FIELD: u32 = 1010;
/// Super-boom builtin arity.
pub const E1011_SUPER_BOOM_ARITY: u32 = 1011;
/// JSON builtin arity error.
pub const E1012_JSON_ARITY: u32 = 1012;
/// JSON parse error.
pub const E1013_JSON_PARSE: u32 = 1013;
/// JSON stringify / unsupported type error.
pub const E1014_JSON_TYPE: u32 = 1014;

/// Codec builtin arity error.
pub const E1030_CODEC_ARITY: u32 = 1030;
/// Codec encode/decode error.
pub const E1031_CODEC_ERROR: u32 = 1031;

/// Crypto builtin arity error.
pub const E1040_CRYPTO_ARITY: u32 = 1040;
/// Crypto operation error.
pub const E1041_CRYPTO_ERROR: u32 = 1041;

/// Unknown class name.
pub const E1020_UNKNOWN_CLASS: u32 = 1020;
/// Unknown method on class or instance.
pub const E1021_UNKNOWN_METHOD: u32 = 1021;
/// Trait not implemented by class.
pub const E1022_TRAIT_NOT_IMPL: u32 = 1022;
/// Invalid `super` call.
pub const E1023_INVALID_SUPER: u32 = 1023;
/// Private member access denied.
pub const E1024_PRIVATE_ACCESS: u32 = 1024;
/// Static/instance call mismatch.
pub const E1025_CALL_KIND: u32 = 1025;

/// DSA builtin arity error.
pub const E1100_DSA_ARITY: u32 = 1100;
/// DSA index out of bounds.
pub const E1101_DSA_BOUNDS: u32 = 1101;
/// DSA graph node out of range.
pub const E1102_DSA_GRAPH: u32 = 1102;

/// I/O builtin arity error.
pub const E1200_IO_ARITY: u32 = 1200;
/// I/O operation failed.
pub const E1201_IO_ERROR: u32 = 1201;
/// Invalid or closed file handle.
pub const E1202_IO_INVALID_HANDLE: u32 = 1202;
/// Async I/O task not found.
pub const E1203_IO_TASK_NOT_FOUND: u32 = 1203;

/// Regex builtin arity error.
pub const E1300_RE_ARITY: u32 = 1300;
/// Invalid regex pattern.
pub const E1301_RE_PATTERN: u32 = 1301;
/// Invalid or closed regex handle.
pub const E1302_RE_INVALID_HANDLE: u32 = 1302;

/// Net builtin arity error.
pub const E1400_NET_ARITY: u32 = 1400;
/// Net operation failed (connection, protocol).
pub const E1401_NET_ERROR: u32 = 1401;
/// Invalid socket or net handle.
pub const E1402_NET_INVALID_HANDLE: u32 = 1402;
/// Invalid URL.
pub const E1403_NET_URL: u32 = 1403;
/// HTTP protocol error.
pub const E1404_NET_HTTP: u32 = 1404;
/// TLS error.
pub const E1405_NET_TLS: u32 = 1405;
/// Async net task not found.
pub const E1406_NET_TASK_NOT_FOUND: u32 = 1406;

/// Parallel builtin arity error.
pub const E1500_PARALLEL_ARITY: u32 = 1500;
/// Parallel lock contention or deadlock.
pub const E1501_PARALLEL_LOCK: u32 = 1501;
/// Parallel channel closed.
pub const E1502_PARALLEL_CHANNEL: u32 = 1502;
/// Invalid parallel handle.
pub const E1503_PARALLEL_INVALID_HANDLE: u32 = 1503;
/// Value is not sendable across threads.
pub const E1504_PARALLEL_NOT_SENDABLE: u32 = 1504;
/// Thread, pool, or task not found.
pub const E1505_PARALLEL_NOT_FOUND: u32 = 1505;

/// Time builtin arity error.
pub const E1600_TIME_ARITY: u32 = 1600;
/// Time operation failed (parse, timezone, invalid date).
pub const E1601_TIME_ERROR: u32 = 1601;

/// nsqlite builtin arity error.
pub const E1700_NSQLITE_ARITY: u32 = 1700;
/// nsqlite SQLite operation failed.
pub const E1701_NSQLITE_ERROR: u32 = 1701;
/// nsqlite invalid or closed handle.
pub const E1702_NSQLITE_INVALID_HANDLE: u32 = 1702;
/// nsqlite constraint or schema error.
pub const E1703_NSQLITE_SCHEMA: u32 = 1703;
/// nsqlite migration error.
pub const E1704_NSQLITE_MIGRATION: u32 = 1704;
/// nsqlite async task not found.
pub const E1705_NSQLITE_TASK_NOT_FOUND: u32 = 1705;
/// nsqlite invalid bind value.
pub const E1706_NSQLITE_BIND: u32 = 1706;

/// nos builtin arity error.
pub const E1800_NOS_ARITY: u32 = 1800;
/// nos OS operation failed.
pub const E1801_NOS_ERROR: u32 = 1801;

/// npg builtin arity error.
pub const E1900_NPG_ARITY: u32 = 1900;
/// npg PostgreSQL operation failed.
pub const E1901_NPG_ERROR: u32 = 1901;
/// npg invalid or closed handle.
pub const E1902_NPG_INVALID_HANDLE: u32 = 1902;
/// npg schema or constraint error.
pub const E1903_NPG_SCHEMA: u32 = 1903;
/// npg migration error.
pub const E1904_NPG_MIGRATION: u32 = 1904;
/// npg async task not found.
pub const E1905_NPG_TASK_NOT_FOUND: u32 = 1905;
/// npg invalid bind value.
pub const E1906_NPG_BIND: u32 = 1906;
/// npg TLS or connection error.
pub const E1907_NPG_TLS: u32 = 1907;

/// ahiru builtin arity error.
pub const E2100_AHIRU_ARITY: u32 = 2100;
/// ahiru server operation failed.
pub const E2101_AHIRU_ERROR: u32 = 2101;
/// ahiru invalid app handle.
pub const E2102_AHIRU_INVALID_HANDLE: u32 = 2102;
/// ahiru state key missing.
pub const E2110_AHIRU_STATE_MISSING: u32 = 2110;
/// ahiru invalid route group handle.
pub const E2111_AHIRU_INVALID_GROUP: u32 = 2111;
/// ahiru validation failed.
pub const E2120_AHIRU_VALIDATION: u32 = 2120;
/// ahiru stream closed.
pub const E2130_AHIRU_STREAM_CLOSED: u32 = 2130;
/// ahiru job enqueue failed.
pub const E2200_AHIRU_JOB_ENQUEUE: u32 = 2200;
/// ahiru cron parse error.
pub const E2201_AHIRU_CRON_PARSE: u32 = 2201;
/// ahiru cache miss.
pub const E2300_AHIRU_CACHE_MISS: u32 = 2300;
/// ahiru redis unavailable.
pub const E2301_AHIRU_REDIS_UNAVAILABLE: u32 = 2301;
/// ahiru oauth state mismatch.
pub const E2400_AHIRU_OAUTH_STATE: u32 = 2400;
/// ahiru mfa required.
pub const E2401_AHIRU_MFA_REQUIRED: u32 = 2401;
/// ahiru websocket room not found.
pub const E2500_AHIRU_WS_ROOM: u32 = 2500;

/// nmongo builtin arity error.
pub const E1920_NMONGO_ARITY: u32 = 1920;
/// nmongo MongoDB operation failed.
pub const E1921_NMONGO_ERROR: u32 = 1921;
/// nmongo invalid or closed handle.
pub const E1922_NMONGO_INVALID_HANDLE: u32 = 1922;
/// nmongo invalid database/collection name.
pub const E1923_NMONGO_INVALID_NAME: u32 = 1923;
/// nmongo BSON type conversion error.
pub const E1924_NMONGO_BSON: u32 = 1924;
/// nmongo async task not found.
pub const E1925_NMONGO_TASK_NOT_FOUND: u32 = 1925;
/// nmongo transaction state error.
pub const E1926_NMONGO_TRANSACTION: u32 = 1926;
/// nmongo GridFS error.
pub const E1927_NMONGO_GRIDFS: u32 = 1927;
/// nmongo change stream error.
pub const E1928_NMONGO_CHANGE_STREAM: u32 = 1928;

/// nenv builtin arity error.
pub const E1950_NENV_ARITY: u32 = 1950;
/// nenv parse/load/IO failure.
pub const E1951_NENV_ERROR: u32 = 1951;
/// nenv required variable not found.
pub const E1952_NENV_NOT_FOUND: u32 = 1952;
/// nenv typed getter or validate type mismatch.
pub const E1953_NENV_INVALID_VALUE: u32 = 1953;
/// nenv invalid store handle.
pub const E1954_NENV_INVALID_HANDLE: u32 = 1954;

/// ncl builtin arity error.
pub const E1960_NCL_ARITY: u32 = 1960;
/// ncl operation failed.
pub const E1961_NCL_ERROR: u32 = 1961;
/// ncl invalid or closed handle.
pub const E1962_NCL_INVALID_HANDLE: u32 = 1962;
/// ncl index out of bounds.
pub const E1963_NCL_BOUNDS: u32 = 1963;
/// ncl type mismatch.
pub const E1964_NCL_TYPE: u32 = 1964;
/// ncl shape error.
pub const E1965_NCL_SHAPE: u32 = 1965;

/// nml builtin arity error.
pub const E1970_NML_ARITY: u32 = 1970;
/// nml operation failed.
pub const E1971_NML_ERROR: u32 = 1971;
/// nml invalid or closed handle.
pub const E1972_NML_INVALID_HANDLE: u32 = 1972;
/// nml shape error.
pub const E1973_NML_SHAPE: u32 = 1973;
/// nml type mismatch.
pub const E1974_NML_TYPE: u32 = 1974;
/// nml device error.
pub const E1975_NML_DEVICE: u32 = 1975;

/// nrag builtin arity error.
pub const E1980_NRAG_ARITY: u32 = 1980;
/// nrag operation failed.
pub const E1981_NRAG_ERROR: u32 = 1981;
/// nrag invalid or closed handle.
pub const E1982_NRAG_INVALID_HANDLE: u32 = 1982;

/// nllm builtin arity error.
pub const E1985_NLLM_ARITY: u32 = 1985;
/// nllm operation failed.
pub const E1986_NLLM_ERROR: u32 = 1986;
/// nllm invalid or closed handle.
pub const E1987_NLLM_INVALID_HANDLE: u32 = 1987;

/// nstr builtin arity error.
pub const E2600_NSTR_ARITY: u32 = 2600;
/// nstr operation failed.
pub const E2601_NSTR_ERROR: u32 = 2601;
/// nstr type mismatch.
pub const E2602_NSTR_TYPE: u32 = 2602;
/// nstr index out of bounds.
pub const E2603_NSTR_BOUNDS: u32 = 2603;

/// nmath builtin arity error.
pub const E2610_NMATH_ARITY: u32 = 2610;
/// nmath operation failed.
pub const E2611_NMATH_ERROR: u32 = 2611;
/// nmath type mismatch.
pub const E2612_NMATH_TYPE: u32 = 2612;
/// nmath domain error (e.g. sqrt of negative, empty stats input).
pub const E2613_NMATH_DOMAIN: u32 = 2613;

/// nrand builtin arity error.
pub const E2620_NRAND_ARITY: u32 = 2620;
/// nrand operation failed.
pub const E2621_NRAND_ERROR: u32 = 2621;
/// nrand type mismatch.
pub const E2622_NRAND_TYPE: u32 = 2622;
/// nrand invalid or closed generator handle.
pub const E2623_NRAND_INVALID_HANDLE: u32 = 2623;

/// nfmt builtin arity error.
pub const E2630_NFMT_ARITY: u32 = 2630;
/// nfmt formatting failed.
pub const E2631_NFMT_ERROR: u32 = 2631;
/// nfmt type mismatch.
pub const E2632_NFMT_TYPE: u32 = 2632;

/// nlog builtin arity error.
pub const E2640_NLOG_ARITY: u32 = 2640;
/// nlog operation failed (sink I/O, bad level).
pub const E2641_NLOG_ERROR: u32 = 2641;
/// nlog type mismatch.
pub const E2642_NLOG_TYPE: u32 = 2642;

/// nargs builtin arity error.
pub const E2650_NARGS_ARITY: u32 = 2650;
/// nargs argv parse error (unknown flag, missing value, bad type).
pub const E2651_NARGS_PARSE: u32 = 2651;
/// nargs invalid spec object.
pub const E2652_NARGS_SPEC: u32 = 2652;

/// ntest builtin arity error.
pub const E2660_NTEST_ARITY: u32 = 2660;
/// ntest runner error.
pub const E2661_NTEST_ERROR: u32 = 2661;
/// ntest assertion failed.
pub const E2662_NTEST_ASSERT: u32 = 2662;

/// ncache builtin arity error.
pub const E2670_NCACHE_ARITY: u32 = 2670;
/// ncache operation failed.
pub const E2671_NCACHE_ERROR: u32 = 2671;
/// ncache invalid or closed cache handle.
pub const E2672_NCACHE_INVALID_HANDLE: u32 = 2672;

/// nvalid builtin arity error.
pub const E2680_NVALID_ARITY: u32 = 2680;
/// nvalid validation engine error.
pub const E2681_NVALID_ERROR: u32 = 2681;
/// nvalid invalid schema object.
pub const E2682_NVALID_SCHEMA: u32 = 2682;

/// ncolor builtin arity error.
pub const E2690_NCOLOR_ARITY: u32 = 2690;
/// ncolor type mismatch or unknown color.
pub const E2691_NCOLOR_TYPE: u32 = 2691;

/// Division by zero.
pub const E2001_DIVISION_BY_ZERO: u32 = 2001;
/// Reference to undefined variable.
pub const E2002_UNDEFINED_VAR: u32 = 2002;
/// Type mismatch or invalid operation.
pub const E2003_TYPE_ERROR: u32 = 2003;
/// Failed assertion.
pub const E2004_ASSERT_FAILED: u32 = 2004;
/// Module file not found.
pub const E2005_MODULE_NOT_FOUND: u32 = 2005;
/// Circular import detected.
pub const E2006_IMPORT_CYCLE: u32 = 2006;
/// User-thrown error (`throw` statement).
pub const E2007_THROWN: u32 = 2007;
/// VM stack underflow.
pub const E2008_STACK_UNDERFLOW: u32 = 2008;
/// No `main` function found.
pub const E2009_NO_MAIN: u32 = 2009;

/// Human-readable name for a runtime error kind (used by `type()` and error values).
pub fn runtime_kind_name(code: u32) -> &'static str {
    match code {
        E2001_DIVISION_BY_ZERO => "division_by_zero",
        E2002_UNDEFINED_VAR => "undefined_variable",
        E2003_TYPE_ERROR => "type_error",
        E2004_ASSERT_FAILED => "assert_failed",
        E2005_MODULE_NOT_FOUND => "module_not_found",
        E2006_IMPORT_CYCLE => "import_cycle",
        E2007_THROWN => "thrown",
        E1001_BUILTIN_ARITY..=E1025_CALL_KIND => "builtin_error",
        E1100_DSA_ARITY..=E1102_DSA_GRAPH => "dsa_error",
        E1200_IO_ARITY..=E1203_IO_TASK_NOT_FOUND => "io_error",
        E1300_RE_ARITY..=E1302_RE_INVALID_HANDLE => "re_error",
        E1400_NET_ARITY..=E1406_NET_TASK_NOT_FOUND => "net_error",
        E1500_PARALLEL_ARITY..=E1505_PARALLEL_NOT_FOUND => "parallel_error",
        E1600_TIME_ARITY..=E1601_TIME_ERROR => "time_error",
        E1700_NSQLITE_ARITY..=E1706_NSQLITE_BIND => "nsqlite_error",
        E1800_NOS_ARITY..=E1801_NOS_ERROR => "nos_error",
        E1900_NPG_ARITY..=E1907_NPG_TLS => "npg_error",
        E2100_AHIRU_ARITY..=E2102_AHIRU_INVALID_HANDLE => "ahiru_error",
        E1920_NMONGO_ARITY..=E1928_NMONGO_CHANGE_STREAM => "nmongo_error",
        E1950_NENV_ARITY..=E1954_NENV_INVALID_HANDLE => "nenv_error",
        E1960_NCL_ARITY..=E1965_NCL_SHAPE => "ncl_error",
        E1970_NML_ARITY..=E1975_NML_DEVICE => "nml_error",
        E1980_NRAG_ARITY..=E1982_NRAG_INVALID_HANDLE => "nrag_error",
        E1985_NLLM_ARITY..=E1987_NLLM_INVALID_HANDLE => "nllm_error",
        E2600_NSTR_ARITY..=E2603_NSTR_BOUNDS => "nstr_error",
        E2610_NMATH_ARITY..=E2613_NMATH_DOMAIN => "nmath_error",
        E2620_NRAND_ARITY..=E2623_NRAND_INVALID_HANDLE => "nrand_error",
        E2630_NFMT_ARITY..=E2632_NFMT_TYPE => "nfmt_error",
        E2640_NLOG_ARITY..=E2642_NLOG_TYPE => "nlog_error",
        E2650_NARGS_ARITY..=E2652_NARGS_SPEC => "nargs_error",
        E2660_NTEST_ARITY..=E2662_NTEST_ASSERT => "ntest_error",
        E2670_NCACHE_ARITY..=E2672_NCACHE_INVALID_HANDLE => "ncache_error",
        E2680_NVALID_ARITY..=E2682_NVALID_SCHEMA => "nvalid_error",
        E2690_NCOLOR_ARITY..=E2691_NCOLOR_TYPE => "ncolor_error",
        _ => "runtime_error",
    }
}
