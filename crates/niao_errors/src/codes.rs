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

// --- expansion, cloud, unique, advanced batches (2700-3439) ---
/// e2700 ncpu arity.
pub const E2700_NCPU_ARITY: u32 = 2700;

/// e2701 ncpu error.
pub const E2701_NCPU_ERROR: u32 = 2701;

/// e2702 ncpu type.
pub const E2702_NCPU_TYPE: u32 = 2702;

/// e2710 ngpu arity.
pub const E2710_NGPU_ARITY: u32 = 2710;

/// e2711 ngpu error.
pub const E2711_NGPU_ERROR: u32 = 2711;

/// e2712 ngpu type.
pub const E2712_NGPU_TYPE: u32 = 2712;

/// e2713 ngpu unavailable.
pub const E2713_NGPU_UNAVAILABLE: u32 = 2713;

/// e2720 nram arity.
pub const E2720_NRAM_ARITY: u32 = 2720;

/// e2721 nram error.
pub const E2721_NRAM_ERROR: u32 = 2721;

/// e2722 nram type.
pub const E2722_NRAM_TYPE: u32 = 2722;

/// e2730 nnpu arity.
pub const E2730_NNPU_ARITY: u32 = 2730;

/// e2731 nnpu error.
pub const E2731_NNPU_ERROR: u32 = 2731;

/// e2732 nnpu type.
pub const E2732_NNPU_TYPE: u32 = 2732;

/// e2740 ndevice arity.
pub const E2740_NDEVICE_ARITY: u32 = 2740;

/// e2741 ndevice error.
pub const E2741_NDEVICE_ERROR: u32 = 2741;

/// e2742 ndevice type.
pub const E2742_NDEVICE_TYPE: u32 = 2742;

/// e2743 ndevice throttle.
pub const E2743_NDEVICE_THROTTLE: u32 = 2743;

/// e2760 neval arity.
pub const E2760_NEVAL_ARITY: u32 = 2760;

/// e2761 neval error.
pub const E2761_NEVAL_ERROR: u32 = 2761;

/// e2762 neval type.
pub const E2762_NEVAL_TYPE: u32 = 2762;

/// e2763 neval shape.
pub const E2763_NEVAL_SHAPE: u32 = 2763;

/// e2770 ntok arity.
pub const E2770_NTOK_ARITY: u32 = 2770;

/// e2771 ntok error.
pub const E2771_NTOK_ERROR: u32 = 2771;

/// e2772 ntok type.
pub const E2772_NTOK_TYPE: u32 = 2772;

/// e2773 ntok invalid handle.
pub const E2773_NTOK_INVALID_HANDLE: u32 = 2773;

/// e2780 nredis arity.
pub const E2780_NREDIS_ARITY: u32 = 2780;

/// e2781 nredis error.
pub const E2781_NREDIS_ERROR: u32 = 2781;

/// e2782 nredis type.
pub const E2782_NREDIS_TYPE: u32 = 2782;

/// e2783 nredis invalid handle.
pub const E2783_NREDIS_INVALID_HANDLE: u32 = 2783;

/// e2790 nvec arity.
pub const E2790_NVEC_ARITY: u32 = 2790;

/// e2791 nvec error.
pub const E2791_NVEC_ERROR: u32 = 2791;

/// e2792 nvec type.
pub const E2792_NVEC_TYPE: u32 = 2792;

/// e2793 nvec invalid handle.
pub const E2793_NVEC_INVALID_HANDLE: u32 = 2793;

/// e2800 naws arity.
pub const E2800_NAWS_ARITY: u32 = 2800;

/// e2801 naws error.
pub const E2801_NAWS_ERROR: u32 = 2801;

/// e2802 naws type.
pub const E2802_NAWS_TYPE: u32 = 2802;

/// e2803 naws auth.
pub const E2803_NAWS_AUTH: u32 = 2803;

/// e2810 nazure arity.
pub const E2810_NAZURE_ARITY: u32 = 2810;

/// e2811 nazure error.
pub const E2811_NAZURE_ERROR: u32 = 2811;

/// e2812 nazure type.
pub const E2812_NAZURE_TYPE: u32 = 2812;

/// e2813 nazure auth.
pub const E2813_NAZURE_AUTH: u32 = 2813;

/// e2820 nsupa arity.
pub const E2820_NSUPA_ARITY: u32 = 2820;

/// e2821 nsupa error.
pub const E2821_NSUPA_ERROR: u32 = 2821;

/// e2822 nsupa type.
pub const E2822_NSUPA_TYPE: u32 = 2822;

/// e2823 nsupa auth.
pub const E2823_NSUPA_AUTH: u32 = 2823;

/// e2830 nmodel arity.
pub const E2830_NMODEL_ARITY: u32 = 2830;

/// e2831 nmodel error.
pub const E2831_NMODEL_ERROR: u32 = 2831;

/// e2832 nmodel type.
pub const E2832_NMODEL_TYPE: u32 = 2832;

/// e2833 nmodel schema.
pub const E2833_NMODEL_SCHEMA: u32 = 2833;

/// e2840 ntoml arity.
pub const E2840_NTOML_ARITY: u32 = 2840;

/// e2841 ntoml error.
pub const E2841_NTOML_ERROR: u32 = 2841;

/// e2842 ntoml type.
pub const E2842_NTOML_TYPE: u32 = 2842;

/// e2843 ntoml parse.
pub const E2843_NTOML_PARSE: u32 = 2843;

/// e2850 ncsv arity.
pub const E2850_NCSV_ARITY: u32 = 2850;

/// e2851 ncsv error.
pub const E2851_NCSV_ERROR: u32 = 2851;

/// e2852 ncsv type.
pub const E2852_NCSV_TYPE: u32 = 2852;

/// e2853 ncsv parse.
pub const E2853_NCSV_PARSE: u32 = 2853;

/// e2860 nmarkdown arity.
pub const E2860_NMARKDOWN_ARITY: u32 = 2860;

/// e2861 nmarkdown error.
pub const E2861_NMARKDOWN_ERROR: u32 = 2861;

/// e2862 nmarkdown type.
pub const E2862_NMARKDOWN_TYPE: u32 = 2862;

/// e2870 nws arity.
pub const E2870_NWS_ARITY: u32 = 2870;

/// e2871 nws error.
pub const E2871_NWS_ERROR: u32 = 2871;

/// e2872 nws type.
pub const E2872_NWS_TYPE: u32 = 2872;

/// e2873 nws invalid handle.
pub const E2873_NWS_INVALID_HANDLE: u32 = 2873;

/// e2880 nurl arity.
pub const E2880_NURL_ARITY: u32 = 2880;

/// e2881 nurl error.
pub const E2881_NURL_ERROR: u32 = 2881;

/// e2882 nurl type.
pub const E2882_NURL_TYPE: u32 = 2882;

/// e2890 nsmtp arity.
pub const E2890_NSMTP_ARITY: u32 = 2890;

/// e2891 nsmtp error.
pub const E2891_NSMTP_ERROR: u32 = 2891;

/// e2892 nsmtp type.
pub const E2892_NSMTP_TYPE: u32 = 2892;

/// e2900 nsemver arity.
pub const E2900_NSEMVER_ARITY: u32 = 2900;

/// e2901 nsemver error.
pub const E2901_NSEMVER_ERROR: u32 = 2901;

/// e2902 nsemver parse.
pub const E2902_NSEMVER_PARSE: u32 = 2902;

/// e2910 ncron arity.
pub const E2910_NCRON_ARITY: u32 = 2910;

/// e2911 ncron error.
pub const E2911_NCRON_ERROR: u32 = 2911;

/// e2912 ncron parse.
pub const E2912_NCRON_PARSE: u32 = 2912;

/// e2920 nprompt arity.
pub const E2920_NPROMPT_ARITY: u32 = 2920;

/// e2921 nprompt error.
pub const E2921_NPROMPT_ERROR: u32 = 2921;

/// e2922 nprompt type.
pub const E2922_NPROMPT_TYPE: u32 = 2922;

/// e2930 nshell arity.
pub const E2930_NSHELL_ARITY: u32 = 2930;

/// e2931 nshell error.
pub const E2931_NSHELL_ERROR: u32 = 2931;

/// e2932 nshell type.
pub const E2932_NSHELL_TYPE: u32 = 2932;

/// e2940 nbudget arity.
pub const E2940_NBUDGET_ARITY: u32 = 2940;

/// e2941 nbudget error.
pub const E2941_NBUDGET_ERROR: u32 = 2941;

/// e2942 nbudget type.
pub const E2942_NBUDGET_TYPE: u32 = 2942;

/// e2943 nbudget exceed.
pub const E2943_NBUDGET_EXCEED: u32 = 2943;

/// e2950 ncost arity.
pub const E2950_NCOST_ARITY: u32 = 2950;

/// e2951 ncost error.
pub const E2951_NCOST_ERROR: u32 = 2951;

/// e2952 ncost type.
pub const E2952_NCOST_TYPE: u32 = 2952;

/// e2960 ncassette arity.
pub const E2960_NCASSETTE_ARITY: u32 = 2960;

/// e2961 ncassette error.
pub const E2961_NCASSETTE_ERROR: u32 = 2961;

/// e2962 ncassette type.
pub const E2962_NCASSETTE_TYPE: u32 = 2962;

/// e2963 ncassette invalid handle.
pub const E2963_NCASSETTE_INVALID_HANDLE: u32 = 2963;

/// e2970 nwhy arity.
pub const E2970_NWHY_ARITY: u32 = 2970;

/// e2971 nwhy error.
pub const E2971_NWHY_ERROR: u32 = 2971;

/// e2972 nwhy type.
pub const E2972_NWHY_TYPE: u32 = 2972;

/// e2973 nwhy invalid handle.
pub const E2973_NWHY_INVALID_HANDLE: u32 = 2973;

/// e2980 ncap arity.
pub const E2980_NCAP_ARITY: u32 = 2980;

/// e2981 ncap error.
pub const E2981_NCAP_ERROR: u32 = 2981;

/// e2982 ncap type.
pub const E2982_NCAP_TYPE: u32 = 2982;

/// e2983 ncap denied.
pub const E2983_NCAP_DENIED: u32 = 2983;

/// e2990 nagent arity.
pub const E2990_NAGENT_ARITY: u32 = 2990;

/// e2991 nagent error.
pub const E2991_NAGENT_ERROR: u32 = 2991;

/// e2992 nagent type.
pub const E2992_NAGENT_TYPE: u32 = 2992;

/// e2993 nagent invalid handle.
pub const E2993_NAGENT_INVALID_HANDLE: u32 = 2993;

/// e3000 nsketch arity.
pub const E3000_NSKETCH_ARITY: u32 = 3000;

/// e3001 nsketch error.
pub const E3001_NSKETCH_ERROR: u32 = 3001;

/// e3002 nsketch type.
pub const E3002_NSKETCH_TYPE: u32 = 3002;

/// e3003 nsketch invalid handle.
pub const E3003_NSKETCH_INVALID_HANDLE: u32 = 3003;

/// e3010 nexplain arity.
pub const E3010_NEXPLAIN_ARITY: u32 = 3010;

/// e3011 nexplain error.
pub const E3011_NEXPLAIN_ERROR: u32 = 3011;

/// e3012 nexplain type.
pub const E3012_NEXPLAIN_TYPE: u32 = 3012;

/// e3020 npace arity.
pub const E3020_NPACE_ARITY: u32 = 3020;

/// e3021 npace error.
pub const E3021_NPACE_ERROR: u32 = 3021;

/// e3022 npace type.
pub const E3022_NPACE_TYPE: u32 = 3022;

/// e3030 nbatch arity.
pub const E3030_NBATCH_ARITY: u32 = 3030;

/// e3031 nbatch error.
pub const E3031_NBATCH_ERROR: u32 = 3031;

/// e3032 nbatch type.
pub const E3032_NBATCH_TYPE: u32 = 3032;

/// e3040 nfallback arity.
pub const E3040_NFALLBACK_ARITY: u32 = 3040;

/// e3041 nfallback error.
pub const E3041_NFALLBACK_ERROR: u32 = 3041;

/// e3042 nfallback type.
pub const E3042_NFALLBACK_TYPE: u32 = 3042;

/// e3050 nmem arity.
pub const E3050_NMEM_ARITY: u32 = 3050;

/// e3051 nmem error.
pub const E3051_NMEM_ERROR: u32 = 3051;

/// e3052 nmem type.
pub const E3052_NMEM_TYPE: u32 = 3052;

/// e3053 nmem invalid handle.
pub const E3053_NMEM_INVALID_HANDLE: u32 = 3053;

/// e3060 ndiff arity.
pub const E3060_NDIFF_ARITY: u32 = 3060;

/// e3061 ndiff error.
pub const E3061_NDIFF_ERROR: u32 = 3061;

/// e3062 ndiff type.
pub const E3062_NDIFF_TYPE: u32 = 3062;

/// e3070 ncanon arity.
pub const E3070_NCANON_ARITY: u32 = 3070;

/// e3071 ncanon error.
pub const E3071_NCANON_ERROR: u32 = 3071;

/// e3072 ncanon type.
pub const E3072_NCANON_TYPE: u32 = 3072;

/// e3080 ncontract arity.
pub const E3080_NCONTRACT_ARITY: u32 = 3080;

/// e3081 ncontract error.
pub const E3081_NCONTRACT_ERROR: u32 = 3081;

/// e3082 ncontract type.
pub const E3082_NCONTRACT_TYPE: u32 = 3082;

/// e3090 nquota arity.
pub const E3090_NQUOTA_ARITY: u32 = 3090;

/// e3091 nquota error.
pub const E3091_NQUOTA_ERROR: u32 = 3091;

/// e3092 nquota type.
pub const E3092_NQUOTA_TYPE: u32 = 3092;

/// e3093 nquota invalid handle.
pub const E3093_NQUOTA_INVALID_HANDLE: u32 = 3093;

/// e3100 nwatch arity.
pub const E3100_NWATCH_ARITY: u32 = 3100;

/// e3101 nwatch error.
pub const E3101_NWATCH_ERROR: u32 = 3101;

/// e3102 nwatch type.
pub const E3102_NWATCH_TYPE: u32 = 3102;

/// e3103 nwatch invalid handle.
pub const E3103_NWATCH_INVALID_HANDLE: u32 = 3103;

/// e3110 nfuzz arity.
pub const E3110_NFUZZ_ARITY: u32 = 3110;

/// e3111 nfuzz error.
pub const E3111_NFUZZ_ERROR: u32 = 3111;

/// e3112 nfuzz type.
pub const E3112_NFUZZ_TYPE: u32 = 3112;

/// e3113 nfuzz invalid handle.
pub const E3113_NFUZZ_INVALID_HANDLE: u32 = 3113;

/// e3120 nshape arity.
pub const E3120_NSHAPE_ARITY: u32 = 3120;

/// e3121 nshape error.
pub const E3121_NSHAPE_ERROR: u32 = 3121;

/// e3122 nshape type.
pub const E3122_NSHAPE_TYPE: u32 = 3122;

/// e3130 npipe arity.
pub const E3130_NPIPE_ARITY: u32 = 3130;

/// e3131 npipe error.
pub const E3131_NPIPE_ERROR: u32 = 3131;

/// e3132 npipe type.
pub const E3132_NPIPE_TYPE: u32 = 3132;

/// e3133 npipe invalid handle.
pub const E3133_NPIPE_INVALID_HANDLE: u32 = 3133;

/// e3140 nreplay arity.
pub const E3140_NREPLAY_ARITY: u32 = 3140;

/// e3141 nreplay error.
pub const E3141_NREPLAY_ERROR: u32 = 3141;

/// e3142 nreplay type.
pub const E3142_NREPLAY_TYPE: u32 = 3142;

/// e3143 nreplay invalid handle.
pub const E3143_NREPLAY_INVALID_HANDLE: u32 = 3143;

/// e3150 nprofile arity.
pub const E3150_NPROFILE_ARITY: u32 = 3150;

/// e3151 nprofile error.
pub const E3151_NPROFILE_ERROR: u32 = 3151;

/// e3152 nprofile type.
pub const E3152_NPROFILE_TYPE: u32 = 3152;

/// e3160 nconfig arity.
pub const E3160_NCONFIG_ARITY: u32 = 3160;

/// e3161 nconfig error.
pub const E3161_NCONFIG_ERROR: u32 = 3161;

/// e3162 nconfig type.
pub const E3162_NCONFIG_TYPE: u32 = 3162;

/// e3163 nconfig missing.
pub const E3163_NCONFIG_MISSING: u32 = 3163;

/// e3170 nbench arity.
pub const E3170_NBENCH_ARITY: u32 = 3170;

/// e3171 nbench error.
pub const E3171_NBENCH_ERROR: u32 = 3171;

/// e3172 nbench type.
pub const E3172_NBENCH_TYPE: u32 = 3172;

/// e3180 ntrace arity.
pub const E3180_NTRACE_ARITY: u32 = 3180;

/// e3181 ntrace error.
pub const E3181_NTRACE_ERROR: u32 = 3181;

/// e3182 ntrace type.
pub const E3182_NTRACE_TYPE: u32 = 3182;

/// e3183 ntrace invalid handle.
pub const E3183_NTRACE_INVALID_HANDLE: u32 = 3183;

/// e3190 ncrash arity.
pub const E3190_NCRASH_ARITY: u32 = 3190;

/// e3191 ncrash error.
pub const E3191_NCRASH_ERROR: u32 = 3191;

/// e3192 ncrash type.
pub const E3192_NCRASH_TYPE: u32 = 3192;

/// e3200 nhotreload arity.
pub const E3200_NHOTRELOAD_ARITY: u32 = 3200;

/// e3201 nhotreload error.
pub const E3201_NHOTRELOAD_ERROR: u32 = 3201;

/// e3202 nhotreload type.
pub const E3202_NHOTRELOAD_TYPE: u32 = 3202;

/// e3203 nhotreload invalid handle.
pub const E3203_NHOTRELOAD_INVALID_HANDLE: u32 = 3203;

/// e3210 ndoc arity.
pub const E3210_NDOC_ARITY: u32 = 3210;

/// e3211 ndoc error.
pub const E3211_NDOC_ERROR: u32 = 3211;

/// e3212 ndoc type.
pub const E3212_NDOC_TYPE: u32 = 3212;

/// e3220 nlint arity.
pub const E3220_NLINT_ARITY: u32 = 3220;

/// e3221 nlint error.
pub const E3221_NLINT_ERROR: u32 = 3221;

/// e3222 nlint type.
pub const E3222_NLINT_TYPE: u32 = 3222;

/// e3223 nlint parse.
pub const E3223_NLINT_PARSE: u32 = 3223;

/// e3230 nworkspace arity.
pub const E3230_NWORKSPACE_ARITY: u32 = 3230;

/// e3231 nworkspace error.
pub const E3231_NWORKSPACE_ERROR: u32 = 3231;

/// e3232 nworkspace type.
pub const E3232_NWORKSPACE_TYPE: u32 = 3232;

/// e3240 nerrgen arity.
pub const E3240_NERRGEN_ARITY: u32 = 3240;

/// e3241 nerrgen error.
pub const E3241_NERRGEN_ERROR: u32 = 3241;

/// e3242 nerrgen type.
pub const E3242_NERRGEN_TYPE: u32 = 3242;

/// e3250 nscaffold arity.
pub const E3250_NSCAFFOLD_ARITY: u32 = 3250;

/// e3251 nscaffold error.
pub const E3251_NSCAFFOLD_ERROR: u32 = 3251;

/// e3252 nscaffold type.
pub const E3252_NSCAFFOLD_TYPE: u32 = 3252;

/// e3260 nmigrate arity.
pub const E3260_NMIGRATE_ARITY: u32 = 3260;

/// e3261 nmigrate error.
pub const E3261_NMIGRATE_ERROR: u32 = 3261;

/// e3262 nmigrate type.
pub const E3262_NMIGRATE_TYPE: u32 = 3262;

/// e3270 nrepl arity.
pub const E3270_NREPL_ARITY: u32 = 3270;

/// e3271 nrepl error.
pub const E3271_NREPL_ERROR: u32 = 3271;

/// e3272 nrepl type.
pub const E3272_NREPL_TYPE: u32 = 3272;

/// e3280 ndebug arity.
pub const E3280_NDEBUG_ARITY: u32 = 3280;

/// e3281 ndebug error.
pub const E3281_NDEBUG_ERROR: u32 = 3281;

/// e3282 ndebug type.
pub const E3282_NDEBUG_TYPE: u32 = 3282;

/// e3283 ndebug invalid handle.
pub const E3283_NDEBUG_INVALID_HANDLE: u32 = 3283;

/// e3290 nschema arity.
pub const E3290_NSCHEMA_ARITY: u32 = 3290;

/// e3291 nschema error.
pub const E3291_NSCHEMA_ERROR: u32 = 3291;

/// e3292 nschema type.
pub const E3292_NSCHEMA_TYPE: u32 = 3292;

/// e3293 nschema validate.
pub const E3293_NSCHEMA_VALIDATE: u32 = 3293;

/// e3300 ntemplate arity.
pub const E3300_NTEMPLATE_ARITY: u32 = 3300;

/// e3301 ntemplate error.
pub const E3301_NTEMPLATE_ERROR: u32 = 3301;

/// e3302 ntemplate type.
pub const E3302_NTEMPLATE_TYPE: u32 = 3302;

/// e3310 nembed arity.
pub const E3310_NEMBED_ARITY: u32 = 3310;

/// e3311 nembed error.
pub const E3311_NEMBED_ERROR: u32 = 3311;

/// e3312 nembed type.
pub const E3312_NEMBED_TYPE: u32 = 3312;

/// e3313 nembed invalid handle.
pub const E3313_NEMBED_INVALID_HANDLE: u32 = 3313;

/// e3320 nguard arity.
pub const E3320_NGUARD_ARITY: u32 = 3320;

/// e3321 nguard error.
pub const E3321_NGUARD_ERROR: u32 = 3321;

/// e3322 nguard type.
pub const E3322_NGUARD_TYPE: u32 = 3322;

/// e3330 nprovider arity.
pub const E3330_NPROVIDER_ARITY: u32 = 3330;

/// e3331 nprovider error.
pub const E3331_NPROVIDER_ERROR: u32 = 3331;

/// e3332 nprovider type.
pub const E3332_NPROVIDER_TYPE: u32 = 3332;

/// e3340 nctx arity.
pub const E3340_NCTX_ARITY: u32 = 3340;

/// e3341 nctx error.
pub const E3341_NCTX_ERROR: u32 = 3341;

/// e3342 nctx type.
pub const E3342_NCTX_TYPE: u32 = 3342;

/// e3350 nsimd arity.
pub const E3350_NSIMD_ARITY: u32 = 3350;

/// e3351 nsimd error.
pub const E3351_NSIMD_ERROR: u32 = 3351;

/// e3352 nsimd type.
pub const E3352_NSIMD_TYPE: u32 = 3352;

/// e3360 nmmap arity.
pub const E3360_NMMAP_ARITY: u32 = 3360;

/// e3361 nmmap error.
pub const E3361_NMMAP_ERROR: u32 = 3361;

/// e3362 nmmap type.
pub const E3362_NMMAP_TYPE: u32 = 3362;

/// e3363 nmmap invalid handle.
pub const E3363_NMMAP_INVALID_HANDLE: u32 = 3363;

/// e3370 narena arity.
pub const E3370_NARENA_ARITY: u32 = 3370;

/// e3371 narena error.
pub const E3371_NARENA_ERROR: u32 = 3371;

/// e3372 narena type.
pub const E3372_NARENA_TYPE: u32 = 3372;

/// e3373 narena invalid handle.
pub const E3373_NARENA_INVALID_HANDLE: u32 = 3373;

/// e3380 nsoa arity.
pub const E3380_NSOA_ARITY: u32 = 3380;

/// e3381 nsoa error.
pub const E3381_NSOA_ERROR: u32 = 3381;

/// e3382 nsoa type.
pub const E3382_NSOA_TYPE: u32 = 3382;

/// e3383 nsoa invalid handle.
pub const E3383_NSOA_INVALID_HANDLE: u32 = 3383;

/// e3390 npar arity.
pub const E3390_NPAR_ARITY: u32 = 3390;

/// e3391 npar error.
pub const E3391_NPAR_ERROR: u32 = 3391;

/// e3392 npar type.
pub const E3392_NPAR_TYPE: u32 = 3392;

/// e3400 npersist arity.
pub const E3400_NPERSIST_ARITY: u32 = 3400;

/// e3401 npersist error.
pub const E3401_NPERSIST_ERROR: u32 = 3401;

/// e3402 npersist type.
pub const E3402_NPERSIST_TYPE: u32 = 3402;

/// e3403 npersist invalid handle.
pub const E3403_NPERSIST_INVALID_HANDLE: u32 = 3403;

/// e3410 nlazy arity.
pub const E3410_NLAZY_ARITY: u32 = 3410;

/// e3411 nlazy error.
pub const E3411_NLAZY_ERROR: u32 = 3411;

/// e3412 nlazy type.
pub const E3412_NLAZY_TYPE: u32 = 3412;

/// e3413 nlazy invalid handle.
pub const E3413_NLAZY_INVALID_HANDLE: u32 = 3413;

/// e3420 nsnap arity.
pub const E3420_NSNAP_ARITY: u32 = 3420;

/// e3421 nsnap error.
pub const E3421_NSNAP_ERROR: u32 = 3421;

/// e3422 nsnap type.
pub const E3422_NSNAP_TYPE: u32 = 3422;

/// e3423 nsnap format.
pub const E3423_NSNAP_FORMAT: u32 = 3423;

/// e3430 ncolumnar arity.
pub const E3430_NCOLUMNAR_ARITY: u32 = 3430;

/// e3431 ncolumnar error.
pub const E3431_NCOLUMNAR_ERROR: u32 = 3431;

/// e3432 ncolumnar type.
pub const E3432_NCOLUMNAR_TYPE: u32 = 3432;

/// e3433 ncolumnar format.
pub const E3433_NCOLUMNAR_FORMAT: u32 = 3433;

/// e4000 nnum arity.
pub const E4000_NNUM_ARITY: u32 = 4000;
/// e4001 nnum error.
pub const E4001_NNUM_ERROR: u32 = 4001;
/// e4002 nnum type.
pub const E4002_NNUM_TYPE: u32 = 4002;
/// e4003 nnum shape mismatch.
pub const E4003_NNUM_SHAPE: u32 = 4003;
/// e4004 nnum singular matrix.
pub const E4004_NNUM_SINGULAR: u32 = 4004;
/// e4005 nnum non-convergence.
pub const E4005_NNUM_NON_CONVERGENCE: u32 = 4005;

/// e4010 nframe arity.
pub const E4010_NFRAME_ARITY: u32 = 4010;
/// e4011 nframe error.
pub const E4011_NFRAME_ERROR: u32 = 4011;
/// e4012 nframe type.
pub const E4012_NFRAME_TYPE: u32 = 4012;
/// e4013 nframe bad column.
pub const E4013_NFRAME_COLUMN: u32 = 4013;
/// e4014 nframe length mismatch.
pub const E4014_NFRAME_LENGTH: u32 = 4014;
/// e4015 nframe dtype.
pub const E4015_NFRAME_DTYPE: u32 = 4015;

/// e4020 nstats arity.
pub const E4020_NSTATS_ARITY: u32 = 4020;
/// e4021 nstats error.
pub const E4021_NSTATS_ERROR: u32 = 4021;
/// e4022 nstats type.
pub const E4022_NSTATS_TYPE: u32 = 4022;
/// e4023 nstats domain.
pub const E4023_NSTATS_DOMAIN: u32 = 4023;
/// e4024 nstats non-convergence.
pub const E4024_NSTATS_NON_CONVERGENCE: u32 = 4024;

/// e4030 noptim arity.
pub const E4030_NOPTIM_ARITY: u32 = 4030;
/// e4031 noptim error.
pub const E4031_NOPTIM_ERROR: u32 = 4031;
/// e4032 noptim type.
pub const E4032_NOPTIM_TYPE: u32 = 4032;
/// e4033 noptim non-convergence.
pub const E4033_NOPTIM_NON_CONVERGENCE: u32 = 4033;
/// e4034 noptim bad bounds.
pub const E4034_NOPTIM_BOUNDS: u32 = 4034;
/// e4035 noptim infeasible.
pub const E4035_NOPTIM_INFEASIBLE: u32 = 4035;

/// e4040 nplot arity.
pub const E4040_NPLOT_ARITY: u32 = 4040;
/// e4041 nplot error.
pub const E4041_NPLOT_ERROR: u32 = 4041;
/// e4042 nplot type.
pub const E4042_NPLOT_TYPE: u32 = 4042;
/// e4043 nplot invalid handle.
pub const E4043_NPLOT_HANDLE: u32 = 4043;
/// e4044 nplot render/encode.
pub const E4044_NPLOT_RENDER: u32 = 4044;

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
        E2990_NAGENT_ARITY..=E2993_NAGENT_INVALID_HANDLE => "nagent_error",
        E3370_NARENA_ARITY..=E3373_NARENA_INVALID_HANDLE => "narena_error",
        E2800_NAWS_ARITY..=E2803_NAWS_AUTH => "naws_error",
        E2810_NAZURE_ARITY..=E2813_NAZURE_AUTH => "nazure_error",
        E3030_NBATCH_ARITY..=E3032_NBATCH_TYPE => "nbatch_error",
        E3170_NBENCH_ARITY..=E3172_NBENCH_TYPE => "nbench_error",
        E2940_NBUDGET_ARITY..=E2943_NBUDGET_EXCEED => "nbudget_error",
        E3070_NCANON_ARITY..=E3072_NCANON_TYPE => "ncanon_error",
        E2980_NCAP_ARITY..=E2983_NCAP_DENIED => "ncap_error",
        E2960_NCASSETTE_ARITY..=E2963_NCASSETTE_INVALID_HANDLE => "ncassette_error",
        E3430_NCOLUMNAR_ARITY..=E3433_NCOLUMNAR_FORMAT => "ncolumnar_error",
        E3160_NCONFIG_ARITY..=E3163_NCONFIG_MISSING => "nconfig_error",
        E3080_NCONTRACT_ARITY..=E3082_NCONTRACT_TYPE => "ncontract_error",
        E2950_NCOST_ARITY..=E2952_NCOST_TYPE => "ncost_error",
        E2700_NCPU_ARITY..=E2702_NCPU_TYPE => "ncpu_error",
        E3190_NCRASH_ARITY..=E3192_NCRASH_TYPE => "ncrash_error",
        E2910_NCRON_ARITY..=E2912_NCRON_PARSE => "ncron_error",
        E2850_NCSV_ARITY..=E2853_NCSV_PARSE => "ncsv_error",
        E3340_NCTX_ARITY..=E3342_NCTX_TYPE => "nctx_error",
        E3280_NDEBUG_ARITY..=E3283_NDEBUG_INVALID_HANDLE => "ndebug_error",
        E2740_NDEVICE_ARITY..=E2743_NDEVICE_THROTTLE => "ndevice_error",
        E3060_NDIFF_ARITY..=E3062_NDIFF_TYPE => "ndiff_error",
        E3210_NDOC_ARITY..=E3212_NDOC_TYPE => "ndoc_error",
        E3310_NEMBED_ARITY..=E3313_NEMBED_INVALID_HANDLE => "nembed_error",
        E3240_NERRGEN_ARITY..=E3242_NERRGEN_TYPE => "nerrgen_error",
        E2760_NEVAL_ARITY..=E2763_NEVAL_SHAPE => "neval_error",
        E3010_NEXPLAIN_ARITY..=E3012_NEXPLAIN_TYPE => "nexplain_error",
        E3040_NFALLBACK_ARITY..=E3042_NFALLBACK_TYPE => "nfallback_error",
        E3110_NFUZZ_ARITY..=E3113_NFUZZ_INVALID_HANDLE => "nfuzz_error",
        E2710_NGPU_ARITY..=E2713_NGPU_UNAVAILABLE => "ngpu_error",
        E3320_NGUARD_ARITY..=E3322_NGUARD_TYPE => "nguard_error",
        E3200_NHOTRELOAD_ARITY..=E3203_NHOTRELOAD_INVALID_HANDLE => "nhotreload_error",
        E3410_NLAZY_ARITY..=E3413_NLAZY_INVALID_HANDLE => "nlazy_error",
        E3220_NLINT_ARITY..=E3223_NLINT_PARSE => "nlint_error",
        E2860_NMARKDOWN_ARITY..=E2862_NMARKDOWN_TYPE => "nmarkdown_error",
        E3050_NMEM_ARITY..=E3053_NMEM_INVALID_HANDLE => "nmem_error",
        E3260_NMIGRATE_ARITY..=E3262_NMIGRATE_TYPE => "nmigrate_error",
        E3360_NMMAP_ARITY..=E3363_NMMAP_INVALID_HANDLE => "nmmap_error",
        E2830_NMODEL_ARITY..=E2833_NMODEL_SCHEMA => "nmodel_error",
        E4000_NNUM_ARITY..=E4005_NNUM_NON_CONVERGENCE => "nnum_error",
        E4010_NFRAME_ARITY..=E4015_NFRAME_DTYPE => "nframe_error",
        E4020_NSTATS_ARITY..=E4024_NSTATS_NON_CONVERGENCE => "nstats_error",
        E4030_NOPTIM_ARITY..=E4035_NOPTIM_INFEASIBLE => "noptim_error",
        E4040_NPLOT_ARITY..=E4044_NPLOT_RENDER => "nplot_error",
        E2730_NNPU_ARITY..=E2732_NNPU_TYPE => "nnpu_error",
        E3020_NPACE_ARITY..=E3022_NPACE_TYPE => "npace_error",
        E3390_NPAR_ARITY..=E3392_NPAR_TYPE => "npar_error",
        E3400_NPERSIST_ARITY..=E3403_NPERSIST_INVALID_HANDLE => "npersist_error",
        E3130_NPIPE_ARITY..=E3133_NPIPE_INVALID_HANDLE => "npipe_error",
        E3150_NPROFILE_ARITY..=E3152_NPROFILE_TYPE => "nprofile_error",
        E2920_NPROMPT_ARITY..=E2922_NPROMPT_TYPE => "nprompt_error",
        E3330_NPROVIDER_ARITY..=E3332_NPROVIDER_TYPE => "nprovider_error",
        E3090_NQUOTA_ARITY..=E3093_NQUOTA_INVALID_HANDLE => "nquota_error",
        E2720_NRAM_ARITY..=E2722_NRAM_TYPE => "nram_error",
        E2780_NREDIS_ARITY..=E2783_NREDIS_INVALID_HANDLE => "nredis_error",
        E3270_NREPL_ARITY..=E3272_NREPL_TYPE => "nrepl_error",
        E3140_NREPLAY_ARITY..=E3143_NREPLAY_INVALID_HANDLE => "nreplay_error",
        E3250_NSCAFFOLD_ARITY..=E3252_NSCAFFOLD_TYPE => "nscaffold_error",
        E3290_NSCHEMA_ARITY..=E3293_NSCHEMA_VALIDATE => "nschema_error",
        E2900_NSEMVER_ARITY..=E2902_NSEMVER_PARSE => "nsemver_error",
        E3120_NSHAPE_ARITY..=E3122_NSHAPE_TYPE => "nshape_error",
        E2930_NSHELL_ARITY..=E2932_NSHELL_TYPE => "nshell_error",
        E3350_NSIMD_ARITY..=E3352_NSIMD_TYPE => "nsimd_error",
        E3000_NSKETCH_ARITY..=E3003_NSKETCH_INVALID_HANDLE => "nsketch_error",
        E2890_NSMTP_ARITY..=E2892_NSMTP_TYPE => "nsmtp_error",
        E3420_NSNAP_ARITY..=E3423_NSNAP_FORMAT => "nsnap_error",
        E3380_NSOA_ARITY..=E3383_NSOA_INVALID_HANDLE => "nsoa_error",
        E2820_NSUPA_ARITY..=E2823_NSUPA_AUTH => "nsupa_error",
        E3300_NTEMPLATE_ARITY..=E3302_NTEMPLATE_TYPE => "ntemplate_error",
        E2770_NTOK_ARITY..=E2773_NTOK_INVALID_HANDLE => "ntok_error",
        E2840_NTOML_ARITY..=E2843_NTOML_PARSE => "ntoml_error",
        E3180_NTRACE_ARITY..=E3183_NTRACE_INVALID_HANDLE => "ntrace_error",
        E2880_NURL_ARITY..=E2882_NURL_TYPE => "nurl_error",
        E2790_NVEC_ARITY..=E2793_NVEC_INVALID_HANDLE => "nvec_error",
        E3100_NWATCH_ARITY..=E3103_NWATCH_INVALID_HANDLE => "nwatch_error",
        E2970_NWHY_ARITY..=E2973_NWHY_INVALID_HANDLE => "nwhy_error",
        E3230_NWORKSPACE_ARITY..=E3232_NWORKSPACE_TYPE => "nworkspace_error",
        E2870_NWS_ARITY..=E2873_NWS_INVALID_HANDLE => "nws_error",
        _ => "runtime_error",
    }
}
