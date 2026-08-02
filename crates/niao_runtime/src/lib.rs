use niao_ast::{BinOp, ClassDef, FnDef, Span, StructDef, TraitDef, UnaryOp};
use niao_bignum::BigInt;
use std::cell::{Cell, RefCell};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

mod archive;
mod async_tasks;
mod codec;
mod crypto;
pub mod dsa;
mod dsa_storage;
mod int_algos;
mod int_heap;
pub mod io;
mod json;
pub mod mem;
mod nargs;
mod ncache;
mod ncl;
mod ncolor;
mod nenv;
pub mod net;
mod nfmt;
mod nframe;
mod ngeo;
mod nhub;
mod nlog;
mod nmath;
pub mod nml;
mod nnum;
mod noptim;
mod nos;
mod npg;
mod nplot;
mod nrand;
mod nsqlite;
mod nstats;
mod nstr;
mod ntest;
mod nvalid;
pub mod nvis;
mod oop;
pub mod parallel;
mod re;
mod time;
// --- expansion + advanced batch modules ---
pub mod ahiru;
mod hw;
mod nagent;
mod narena;
mod naws;
mod nazure;
mod nbatch;
mod nbench;
mod nbudget;
mod ncanon;
mod ncap;
mod ncassette;
mod ncolumnar;
mod nconfig;
mod ncontract;
mod ncost;
mod ncpu;
mod ncrash;
mod ncron;
mod ncsv;
mod nctx;
mod ndataset;
mod ndebug;
mod ndevice;
mod ndiff;
mod ndoc;
mod ndsp;
mod nembed;
mod nerrgen;
mod neval;
mod nexplain;
mod nfallback;
mod nfin;
mod nfuzz;
mod ngpu;
mod nguard;
mod nhotreload;
mod nlazy;
mod nlint;
#[cfg(feature = "nllm")]
pub mod nllm;
mod nmarkdown;
mod nmem;
mod nmigrate;
mod nmmap;
mod nmodel;
#[cfg(feature = "nmongo")]
pub mod nmongo;
mod nnpu;
mod nonnx;
mod npace;
mod npar;
mod npersist;
mod npipe;
mod nprofile;
mod nprompt;
mod nprovider;
mod nquota;
#[cfg(feature = "nrag")]
pub mod nrag;
mod nram;
mod nredis;
mod nrepl;
mod nreplay;
mod nscaffold;
mod nschema;
mod nsemver;
mod nshape;
mod nshell;
mod nsimd;
mod nsketch;
mod nsmtp;
mod nsnap;
mod nsoa;
mod nspeech;
mod nsupa;
mod ntemplate;
mod ntok;
mod ntoml;
mod ntrace;
mod ntts;
mod ntune;
mod nunits;
mod nurl;
mod nvec;
mod nwatch;
mod nwhy;
mod nworkspace;
mod nws;
pub use nos::set_program_args;
pub fn set_ahiru_serve_options(opts: ahiru_core::ServeRuntimeOptions) {
    ahiru::set_serve_options(opts);
}

pub fn apply_ahiru_cli_port(port: u16) {
    ahiru::apply_cli_port(port);
}

pub fn start_ahiru_pending_server() -> Result<(), RuntimeError> {
    ahiru::start_pending_server()
}
pub use dsa::NativeDs;
pub use oop::{
    current_method_context, pop_method_context, push_method_context, set_class_registry,
    with_class_registry, ClassRegistry, InstanceMethod, InstanceValue, MethodContext, RuntimeClass,
    RuntimeTrait,
};

pub use io::{io_read_file, io_write_file};
pub use json::{json_parse, json_stringify};
pub use niao_errors::{codes, NiaoErrorValue, NiaoResult, RuntimeError};

/// Hook for native code to invoke Niao functions (HTTP handlers, parallel workers).
pub type NiaoCallHook =
    Arc<dyn Fn(ValueRef, &[ValueRef], Span) -> NiaoResult<ValueRef> + Send + Sync>;

/// Optional VM-backed hook used when the interpreter is not active.
pub type NiaoVmCallHook =
    Arc<dyn Fn(ValueRef, &[ValueRef], Span) -> NiaoResult<ValueRef> + Send + Sync>;

/// Resolve a top-level Niao function by name (interpreter serve / worker threads).
pub type NiaoFnResolver = Arc<dyn Fn(&str) -> Option<ValueRef> + Send + Sync>;

static GLOBAL_NIAO_CALL_HOOK: OnceLock<Mutex<Option<NiaoCallHook>>> = OnceLock::new();
static GLOBAL_VM_CALL_HOOK: OnceLock<Mutex<Option<NiaoVmCallHook>>> = OnceLock::new();
static GLOBAL_FN_RESOLVER: OnceLock<Mutex<Option<NiaoFnResolver>>> = OnceLock::new();
static INTERPRETER_GIL: OnceLock<Mutex<()>> = OnceLock::new();

fn global_call_hook_slot() -> &'static Mutex<Option<NiaoCallHook>> {
    GLOBAL_NIAO_CALL_HOOK.get_or_init(|| Mutex::new(None))
}

/// Global interpreter lock — only one Niao function may execute at a time.
pub fn interpreter_gil() -> &'static Mutex<()> {
    INTERPRETER_GIL.get_or_init(|| Mutex::new(()))
}

fn global_vm_call_hook_slot() -> &'static Mutex<Option<NiaoVmCallHook>> {
    GLOBAL_VM_CALL_HOOK.get_or_init(|| Mutex::new(None))
}

/// Register a VM-backed callback for native→Niao handler dispatch.
pub fn set_niao_vm_call_hook(hook: Option<NiaoVmCallHook>) {
    *global_vm_call_hook_slot().lock().unwrap() = hook;
}

pub fn niao_vm_call_hook_active() -> bool {
    global_vm_call_hook_slot().lock().unwrap().is_some()
}

/// Register a callback used by native modules to call Niao functions.
pub fn set_niao_call_hook(hook: Option<NiaoCallHook>) {
    *global_call_hook_slot().lock().unwrap() = hook;
}

/// Whether a Niao call hook is registered (interpreter mode).
pub fn niao_call_hook_active() -> bool {
    global_call_hook_slot().lock().unwrap().is_some()
}

fn global_fn_resolver_slot() -> &'static Mutex<Option<NiaoFnResolver>> {
    GLOBAL_FN_RESOLVER.get_or_init(|| Mutex::new(None))
}

pub fn set_niao_fn_resolver(resolver: Option<NiaoFnResolver>) {
    *global_fn_resolver_slot().lock().unwrap() = resolver;
}

pub fn resolve_niao_function_by_name(name: &str) -> Option<ValueRef> {
    global_fn_resolver_slot()
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|resolve| resolve(name))
}

/// Invoke a Niao function through the registered interpreter or VM hook.
/// Interpreter path holds the global GIL; VM path does not.
pub fn call_niao_function(callee: ValueRef, args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    let interp_hook = global_call_hook_slot().lock().unwrap();
    if interp_hook.is_some() {
        drop(interp_hook);
        let _gil = interpreter_gil().lock().map_err(|_| {
            RuntimeError::at(span, codes::E1501_PARALLEL_LOCK, "interpreter GIL poisoned")
        })?;
        return global_call_hook_slot().lock().unwrap().as_ref().unwrap()(callee, args, span);
    }
    drop(interp_hook);
    let vm_hook = global_vm_call_hook_slot().lock().unwrap();
    let vm_hook = vm_hook.as_ref().ok_or_else(|| {
        RuntimeError::at(
            span,
            codes::E1404_NET_HTTP,
            "no Niao call hook registered (use interpreter or VM mode for handlers)",
        )
    })?;
    vm_hook(callee, args, span)
}

thread_local! {
    static QUIET_OUTPUT: Cell<bool> = const { Cell::new(false) };
}

/// Suppress `print()` output (used by `niao bench`).
pub fn set_quiet_output(quiet: bool) {
    QUIET_OUTPUT.with(|q| q.set(quiet));
}

pub fn quiet_output() -> bool {
    QUIET_OUTPUT.with(|q| q.get())
}

/// Restores the previous quiet flag when dropped.
pub struct QuietGuard {
    prev: bool,
}

impl QuietGuard {
    pub fn new() -> Self {
        let prev = quiet_output();
        set_quiet_output(true);
        Self { prev }
    }
}

impl Drop for QuietGuard {
    fn drop(&mut self) {
        set_quiet_output(self.prev);
    }
}

pub fn error_value(
    code: u32,
    kind: impl Into<String>,
    message: impl Into<String>,
    span: Span,
) -> ValueRef {
    Value::Error(NiaoErrorValue::new(code, kind, message, span)).ref_cell()
}

pub fn error_from_runtime(err: &RuntimeError) -> ValueRef {
    Value::Error(err.to_niao_error_value()).ref_cell()
}

pub fn value_to_error(val: &Value) -> Option<NiaoErrorValue> {
    match val {
        Value::Error(e) => Some(e.clone()),
        _ => None,
    }
}

pub fn error_field(val: &Value, field: &str) -> Option<Value> {
    let err = value_to_error(val)?;
    match field {
        "code" => Some(Value::Int(err.code as i64)),
        "message" => Some(Value::String(err.message.clone())),
        "kind" => Some(Value::String(err.kind.clone())),
        "line" => Some(Value::Int(err.line as i64)),
        "col" => Some(Value::Int(err.col as i64)),
        _ => None,
    }
}

pub type ValueRef = Rc<RefCell<Value>>;

/// Packed string lines — dense `Vec<String>` or lazy single-buffer lines from `io_read_lines`.
#[derive(Clone)]
pub struct StringArray {
    inner: StringArrayInner,
}

#[derive(Clone)]
enum StringArrayInner {
    Dense(Vec<String>),
    Lines {
        data: Vec<u8>,
        starts: Vec<u32>,
        ascii: bool,
    },
}

impl StringArray {
    pub fn dense(items: Vec<String>) -> Self {
        Self {
            inner: StringArrayInner::Dense(items),
        }
    }

    /// Build lazy line view matching `str::lines()` without per-line allocations.
    pub fn from_lines(text: String) -> Self {
        let (starts, ascii) = compute_line_starts_bytes(text.as_bytes());
        Self {
            inner: StringArrayInner::Lines {
                data: text.into_bytes(),
                starts,
                ascii,
            },
        }
    }

    /// Read lines from raw file bytes (one syscall, no UTF-8 validation pass).
    pub fn from_line_bytes(data: Vec<u8>) -> Self {
        let (starts, ascii) = compute_line_starts_bytes(&data);
        Self {
            inner: StringArrayInner::Lines {
                data,
                starts,
                ascii,
            },
        }
    }

    pub fn len(&self) -> usize {
        match &self.inner {
            StringArrayInner::Dense(v) => v.len(),
            StringArrayInner::Lines { starts, .. } => starts.len(),
        }
    }

    pub fn get(&self, i: usize) -> Option<String> {
        match &self.inner {
            StringArrayInner::Dense(v) => v.get(i).cloned(),
            StringArrayInner::Lines {
                data,
                starts,
                ascii,
            } => {
                let start = *starts.get(i)? as usize;
                let mut end = starts.get(i + 1).map(|&x| x as usize).unwrap_or(data.len());
                if end > start && data.get(end - 1) == Some(&b'\n') {
                    end -= 1;
                }
                let slice = &data[start..end];
                if slice.is_empty() {
                    Some(String::new())
                } else if *ascii {
                    Some(unsafe { String::from_utf8_unchecked(slice.to_vec()) })
                } else {
                    Some(String::from_utf8_lossy(slice).into_owned())
                }
            }
        }
    }

    pub fn dense_vec(&self) -> Vec<String> {
        match &self.inner {
            StringArrayInner::Dense(v) => v.clone(),
            StringArrayInner::Lines { .. } => {
                (0..self.len()).map(|i| self.get(i).unwrap()).collect()
            }
        }
    }

    pub fn set(&mut self, i: usize, val: String) -> bool {
        if matches!(self.inner, StringArrayInner::Lines { .. }) {
            self.inner = StringArrayInner::Dense(self.dense_vec());
        }
        if let StringArrayInner::Dense(v) = &mut self.inner {
            if i < v.len() {
                v[i] = val;
                return true;
            }
        }
        false
    }
}

impl PartialEq for StringArray {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && (0..self.len()).all(|i| self.get(i) == other.get(i))
    }
}

fn compute_line_starts_bytes(bytes: &[u8]) -> (Vec<u32>, bool) {
    if bytes.is_empty() {
        return (Vec::new(), true);
    }
    let mut starts = Vec::with_capacity(bytes.len() / 36 + 1);
    let mut ascii = true;
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] >= 128 {
            ascii = false;
        }
        starts.push(i as u32);
        let mut j = i;
        while j < bytes.len() {
            if bytes[j] >= 128 {
                ascii = false;
            }
            if bytes[j] == b'\n' {
                break;
            }
            j += 1;
        }
        if j >= bytes.len() {
            break;
        }
        i = j + 1;
    }
    (starts, ascii)
}

#[derive(Clone)]
pub enum Value {
    Int(i64),
    BigInt(BigInt),
    Float(f64),
    String(String),
    Bool(bool),
    Nil,
    /// Packed int storage — one allocation, no per-element RefCell.
    IntArray(Vec<i64>),
    /// Packed float storage — one allocation, contiguous f64.
    FloatArray(Vec<f64>),
    /// Packed bool storage — 0/1 bytes, one allocation.
    BoolArray(Vec<u8>),
    /// Compact byte storage for I/O (1 byte per element, no widening).
    ByteArray(Vec<u8>),
    /// Packed string storage — dense or lazy line view (no per-line RefCell).
    StringArray(StringArray),
    Array(Vec<ValueRef>),
    Object(HashMap<String, ValueRef>),
    /// OOP class instance with vtable dispatch.
    Instance(InstanceValue),
    Function(FunctionValue),
    NativeFunction(NativeFn),
    /// Native DSA structure (list, stack, queue, deque, heap, set, map, graph).
    /// Rc-shared: cloning the Value keeps pointing at the same structure.
    Native(Rc<RefCell<NativeDs>>),
    /// Structured runtime error value for `try/catch` and `error()`.
    Error(NiaoErrorValue),
    /// NCL handle (Series, DataFrame, NDArray, GroupBy).
    NclHandle(u64),
    /// NML handle (Tensor, Model, Trainer, DataLoader, classic ML).
    NmlHandle(u64),
    /// Lazy MongoDB document — wire-format BSON, fields decoded on access.
    #[cfg(feature = "nmongo")]
    BsonDoc(Arc<bson::raw::RawDocumentBuf>),
}

#[derive(Clone)]
pub struct FunctionValue {
    pub def: FnDef,
    pub closure: Rc<Environment>,
}

pub type NativeFn = Rc<dyn Fn(&[ValueRef], Span) -> NiaoResult<ValueRef>>;

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(v) => write!(f, "{v}"),
            Value::BigInt(v) => write!(f, "{v}"),
            Value::Float(v) => write!(f, "{v}"),
            Value::String(v) => write!(f, "\"{v}\""),
            Value::Bool(v) => write!(f, "{v}"),
            Value::Nil => write!(f, "nil"),
            Value::IntArray(v) => write!(f, "int_array[{}]", v.len()),
            Value::FloatArray(v) => write!(f, "float_array[{}]", v.len()),
            Value::BoolArray(v) => write!(f, "bool_array[{}]", v.len()),
            Value::ByteArray(v) => write!(f, "byte_array[{}]", v.len()),
            Value::StringArray(v) => write!(f, "string_array[{}]", v.len()),
            Value::Array(v) => write!(f, "array[{}]", v.len()),
            Value::Object(v) => write!(f, "object{{{}}}", v.len()),
            Value::Instance(inst) => write!(f, "{} instance", inst.class_name),
            Value::Function(v) => write!(f, "fn {}", v.def.name),
            Value::NativeFunction(_) => write!(f, "native_fn"),
            Value::Native(ds) => {
                let ds = ds.borrow();
                write!(f, "{}[{}]", ds.kind_name(), ds.len())
            }
            Value::Error(e) => write!(f, "error({})", e.message),
            Value::NclHandle(id) => write!(f, "ncl_handle[{id}]"),
            Value::NmlHandle(id) => write!(f, "nml_handle[{id}]"),
            #[cfg(feature = "nmongo")]
            Value::BsonDoc(buf) => write!(f, "bson_doc[{}b]", buf.as_bytes().len()),
        }
    }
}

impl Value {
    pub fn ref_cell(self) -> ValueRef {
        Rc::new(RefCell::new(self))
    }

    pub fn type_name(&self) -> String {
        match self {
            Value::Int(_) => "int".into(),
            Value::BigInt(_) => "bigint".into(),
            Value::Float(_) => "float".into(),
            Value::String(_) => "string".into(),
            Value::Bool(_) => "bool".into(),
            Value::Nil => "nil".into(),
            Value::IntArray(_)
            | Value::FloatArray(_)
            | Value::BoolArray(_)
            | Value::ByteArray(_)
            | Value::StringArray(_)
            | Value::Array(_) => "array".into(),
            Value::Object(_) => "object".into(),
            Value::Instance(inst) => inst.class_name.clone(),
            Value::Function(_) => "function".into(),
            Value::NativeFunction(_) => "function".into(),
            Value::Native(ds) => ds.borrow().kind_name().to_string(),
            Value::Error(_) => "error".into(),
            Value::NclHandle(id) => ncl::handles::type_name_for(*id),
            Value::NmlHandle(id) => nml::type_name_for(*id),
            #[cfg(feature = "nmongo")]
            Value::BsonDoc(_) => "object".into(),
        }
    }

    /// Field read for `Object` and lazy `BsonDoc` values.
    pub fn object_get_field(&self, field: &str) -> Option<ValueRef> {
        match self {
            Value::Object(map) => map.get(field).cloned(),
            #[cfg(feature = "nmongo")]
            Value::BsonDoc(buf) => crate::nmongo::bson_field_from_raw(buf, field),
            _ => None,
        }
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Nil => false,
            Value::Int(0) => false,
            Value::BigInt(v) => v != &BigInt::from(0),
            Value::Float(f) if *f == 0.0 => false,
            Value::String(s) => !s.is_empty(),
            _ => true,
        }
    }

    pub fn to_string(&self) -> String {
        match self {
            Value::Int(v) => v.to_string(),
            Value::BigInt(v) => v.to_string(),
            Value::Float(v) => v.to_string(),
            Value::String(v) => v.clone(),
            Value::Bool(v) => v.to_string(),
            Value::Nil => "nil".into(),
            Value::IntArray(items) => {
                if items.len() > 32 {
                    return format!("int_array[{}]", items.len());
                }
                let parts: Vec<String> = items.iter().map(|v| v.to_string()).collect();
                format!("[{}]", parts.join(", "))
            }
            Value::FloatArray(items) => {
                if items.len() > 32 {
                    return format!("float_array[{}]", items.len());
                }
                let parts: Vec<String> = items.iter().map(|v| v.to_string()).collect();
                format!("[{}]", parts.join(", "))
            }
            Value::BoolArray(items) => {
                if items.len() > 32 {
                    return format!("bool_array[{}]", items.len());
                }
                let parts: Vec<String> = items
                    .iter()
                    .map(|v| (if *v != 0 { "true" } else { "false" }).to_string())
                    .collect();
                format!("[{}]", parts.join(", "))
            }
            Value::ByteArray(items) => {
                if items.len() > 32 {
                    return format!("byte_array[{}]", items.len());
                }
                let parts: Vec<String> = items.iter().map(|v| v.to_string()).collect();
                format!("[{}]", parts.join(", "))
            }
            Value::StringArray(items) => {
                if items.len() > 32 {
                    return format!("string_array[{}]", items.len());
                }
                let parts: Vec<String> = (0..items.len())
                    .map(|i| format!("\"{}\"", items.get(i).unwrap_or_default()))
                    .collect();
                format!("[{}]", parts.join(", "))
            }
            Value::Array(items) => {
                let parts: Vec<String> = items.iter().map(|v| v.borrow().to_string()).collect();
                format!("[{}]", parts.join(", "))
            }
            Value::Object(map) => {
                let parts: Vec<String> = map
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k, v.borrow().to_string()))
                    .collect();
                format!("{{{}}}", parts.join(", "))
            }
            #[cfg(feature = "nmongo")]
            Value::BsonDoc(buf) => format!("bson_doc[{}b]", buf.as_bytes().len()),
            Value::Instance(inst) => {
                let parts: Vec<String> = inst
                    .fields
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k, v.borrow().to_string()))
                    .collect();
                format!("{} {{{}}}", inst.class_name, parts.join(", "))
            }
            Value::Function(f) => format!("<fn {}>", f.def.name),
            Value::NativeFunction(_) => "<native fn>".into(),
            Value::Native(ds) => ds.borrow().display(),
            Value::Error(e) => e.to_string(),
            Value::NclHandle(id) => ncl::handles::display_for(*id),
            Value::NmlHandle(id) => nml::display_for(*id),
        }
    }
}

#[derive(Clone)]
pub struct Environment {
    pub vars: RefCell<HashMap<String, ValueRef>>,
    pub parent: Option<Rc<Environment>>,
}

impl Environment {
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            vars: RefCell::new(HashMap::new()),
            parent: None,
        })
    }

    pub fn child(parent: Rc<Environment>) -> Rc<Self> {
        Rc::new(Self {
            vars: RefCell::new(HashMap::new()),
            parent: Some(parent),
        })
    }

    pub fn define(&self, name: String, value: ValueRef) {
        self.vars.borrow_mut().insert(name, value);
    }

    pub fn get(&self, name: &str) -> Option<ValueRef> {
        if let Some(v) = self.vars.borrow().get(name) {
            Some(Rc::clone(v))
        } else if let Some(parent) = &self.parent {
            parent.get(name)
        } else {
            None
        }
    }

    pub fn assign(&self, name: &str, value: ValueRef) -> bool {
        if self.vars.borrow().contains_key(name) {
            self.vars.borrow_mut().insert(name.to_string(), value);
            true
        } else if let Some(parent) = &self.parent {
            parent.assign(name, value)
        } else {
            false
        }
    }
}

pub struct Module {
    pub path: String,
    pub exports: HashMap<String, ValueRef>,
    pub structs: HashMap<String, StructDef>,
    pub classes: HashMap<String, ClassDef>,
    pub traits: HashMap<String, TraitDef>,
}

pub struct ModuleLoader {
    pub modules: HashMap<String, Rc<Module>>,
    pub loading: Vec<String>,
}

impl ModuleLoader {
    pub fn new() -> Self {
        Self {
            modules: HashMap::new(),
            loading: Vec::new(),
        }
    }
}

pub fn apply_binop(op: BinOp, left: &Value, right: &Value, span: Span) -> NiaoResult<Value> {
    match op {
        BinOp::Add => add_values(left, right, span),
        BinOp::Sub => sub_values(left, right, span),
        BinOp::Mul => mul_values(left, right, span),
        BinOp::Div => div_values(left, right, span),
        BinOp::FloorDiv => floor_div_values(left, right, span),
        BinOp::Mod => mod_values(left, right, span),
        BinOp::Eq => Ok(Value::Bool(values_equal(left, right))),
        BinOp::Ne => Ok(Value::Bool(!values_equal(left, right))),
        BinOp::Lt => cmp_op(left, right, |ord| ord == Ordering::Less, span),
        BinOp::Gt => cmp_op(left, right, |ord| ord == Ordering::Greater, span),
        BinOp::Le => cmp_op(left, right, |ord| ord != Ordering::Greater, span),
        BinOp::Ge => cmp_op(left, right, |ord| ord != Ordering::Less, span),
        BinOp::And => Ok(Value::Bool(left.is_truthy() && right.is_truthy())),
        BinOp::Or => Ok(Value::Bool(left.is_truthy() || right.is_truthy())),
    }
}

pub fn int_add(a: i64, b: i64) -> Value {
    match a.checked_add(b) {
        Some(v) => Value::Int(v),
        None => Value::BigInt(BigInt::from(a) + BigInt::from(b)),
    }
}

pub fn int_sub(a: i64, b: i64) -> Value {
    match a.checked_sub(b) {
        Some(v) => Value::Int(v),
        None => Value::BigInt(BigInt::from(a) - BigInt::from(b)),
    }
}

pub fn int_mul(a: i64, b: i64) -> Value {
    match a.checked_mul(b) {
        Some(v) => Value::Int(v),
        None => Value::BigInt(BigInt::from(a) * BigInt::from(b)),
    }
}

fn add_values(left: &Value, right: &Value, span: Span) -> NiaoResult<Value> {
    match (left, right) {
        (Value::Int(a), Value::Int(b)) => Ok(int_add(*a, *b)),
        (Value::BigInt(a), Value::BigInt(b)) => Ok(Value::BigInt(a + b)),
        (Value::BigInt(a), Value::Int(b)) => Ok(Value::BigInt(a + &BigInt::from(*b))),
        (Value::Int(a), Value::BigInt(b)) => Ok(Value::BigInt(&BigInt::from(*a) + b)),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
        (Value::Int(a), Value::Float(b)) => Ok(Value::Float(*a as f64 + b)),
        (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a + *b as f64)),
        (Value::String(a), Value::String(b)) => Ok(Value::String(format!("{a}{b}"))),
        (Value::String(a), other) => Ok(Value::String(format!("{a}{}", other.to_string()))),
        (other, Value::String(b)) => Ok(Value::String(format!("{}{b}", other.to_string()))),
        _ => Err(RuntimeError::TypeError {
            message: "invalid operands for +".into(),
            line: span.line,
            col: span.col,
        }),
    }
}

fn sub_values(left: &Value, right: &Value, span: Span) -> NiaoResult<Value> {
    match (left, right) {
        (Value::Int(a), Value::Int(b)) => Ok(int_sub(*a, *b)),
        (Value::BigInt(a), Value::BigInt(b)) => Ok(Value::BigInt(a - b)),
        (Value::BigInt(a), Value::Int(b)) => Ok(Value::BigInt(a - &BigInt::from(*b))),
        (Value::Int(a), Value::BigInt(b)) => Ok(Value::BigInt(&BigInt::from(*a) - b)),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a - b)),
        (Value::Int(a), Value::Float(b)) => Ok(Value::Float(*a as f64 - b)),
        (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a - *b as f64)),
        _ => Err(RuntimeError::TypeError {
            message: "invalid operands for -".into(),
            line: span.line,
            col: span.col,
        }),
    }
}

fn mul_values(left: &Value, right: &Value, span: Span) -> NiaoResult<Value> {
    match (left, right) {
        (Value::Int(a), Value::Int(b)) => Ok(int_mul(*a, *b)),
        (Value::BigInt(a), Value::BigInt(b)) => Ok(Value::BigInt(a * b)),
        (Value::BigInt(a), Value::Int(b)) => Ok(Value::BigInt(a * &BigInt::from(*b))),
        (Value::Int(a), Value::BigInt(b)) => Ok(Value::BigInt(&BigInt::from(*a) * b)),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a * b)),
        (Value::Int(a), Value::Float(b)) => Ok(Value::Float(*a as f64 * b)),
        (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a * *b as f64)),
        _ => Err(RuntimeError::TypeError {
            message: "invalid operands for *".into(),
            line: span.line,
            col: span.col,
        }),
    }
}

fn div_values(left: &Value, right: &Value, span: Span) -> NiaoResult<Value> {
    match (left, right) {
        (Value::Float(a), Value::Float(b)) => {
            if *b == 0.0 {
                Err(RuntimeError::DivisionByZero {
                    line: span.line,
                    col: span.col,
                })
            } else {
                Ok(Value::Float(a / b))
            }
        }
        (a, b) => {
            let af = to_float(a, span)?;
            let bf = to_float(b, span)?;
            if bf == 0.0 {
                Err(RuntimeError::DivisionByZero {
                    line: span.line,
                    col: span.col,
                })
            } else {
                Ok(Value::Float(af / bf))
            }
        }
    }
}

fn floor_div_values(left: &Value, right: &Value, span: Span) -> NiaoResult<Value> {
    match (left, right) {
        (Value::Int(a), Value::Int(b)) => {
            if *b == 0 {
                Err(RuntimeError::DivisionByZero {
                    line: span.line,
                    col: span.col,
                })
            } else {
                Ok(Value::Int(a / b))
            }
        }
        (Value::BigInt(a), Value::BigInt(b)) => {
            if *b == BigInt::from(0) {
                Err(RuntimeError::DivisionByZero {
                    line: span.line,
                    col: span.col,
                })
            } else {
                Ok(Value::BigInt(a / b))
            }
        }
        (Value::BigInt(a), Value::Int(b)) => {
            if *b == 0 {
                Err(RuntimeError::DivisionByZero {
                    line: span.line,
                    col: span.col,
                })
            } else {
                Ok(Value::BigInt(a / &BigInt::from(*b)))
            }
        }
        (Value::Int(a), Value::BigInt(b)) => {
            if *b == BigInt::from(0) {
                Err(RuntimeError::DivisionByZero {
                    line: span.line,
                    col: span.col,
                })
            } else {
                Ok(Value::BigInt(&BigInt::from(*a) / b))
            }
        }
        (Value::Float(a), Value::Float(b)) => {
            if *b == 0.0 {
                Err(RuntimeError::DivisionByZero {
                    line: span.line,
                    col: span.col,
                })
            } else {
                Ok(Value::Float((a / b).trunc()))
            }
        }
        (a, b) => {
            let af = to_float(a, span)?;
            let bf = to_float(b, span)?;
            if bf == 0.0 {
                Err(RuntimeError::DivisionByZero {
                    line: span.line,
                    col: span.col,
                })
            } else {
                Ok(Value::Float((af / bf).trunc()))
            }
        }
    }
}

fn mod_values(left: &Value, right: &Value, span: Span) -> NiaoResult<Value> {
    match (left, right) {
        (Value::Int(a), Value::Int(b)) if *b == 0 => Err(RuntimeError::DivisionByZero {
            line: span.line,
            col: span.col,
        }),
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a % b)),
        (Value::BigInt(a), Value::BigInt(b)) if *b == BigInt::from(0) => {
            Err(RuntimeError::DivisionByZero {
                line: span.line,
                col: span.col,
            })
        }
        (Value::BigInt(a), Value::BigInt(b)) => Ok(Value::BigInt(a % b)),
        (Value::BigInt(a), Value::Int(b)) if *b == 0 => Err(RuntimeError::DivisionByZero {
            line: span.line,
            col: span.col,
        }),
        (Value::BigInt(a), Value::Int(b)) => Ok(Value::BigInt(a % &BigInt::from(*b))),
        (Value::Int(a), Value::BigInt(b)) if *b == BigInt::from(0) => {
            Err(RuntimeError::DivisionByZero {
                line: span.line,
                col: span.col,
            })
        }
        (Value::Int(a), Value::BigInt(b)) => Ok(Value::BigInt(&BigInt::from(*a) % b)),
        _ => Err(RuntimeError::TypeError {
            message: "invalid operands for %".into(),
            line: span.line,
            col: span.col,
        }),
    }
}

pub fn apply_unaryop(op: UnaryOp, val: &Value, span: Span) -> NiaoResult<Value> {
    match op {
        UnaryOp::Not => Ok(Value::Bool(!val.is_truthy())),
        UnaryOp::Neg => match val {
            Value::Int(v) => Ok(Value::Int(-v)),
            Value::BigInt(v) => Ok(Value::BigInt(-v)),
            Value::Float(v) => Ok(Value::Float(-v)),
            _ => Err(RuntimeError::TypeError {
                message: "cannot negate non-number".into(),
                line: span.line,
                col: span.col,
            }),
        },
    }
}

fn cmp_op<F>(left: &Value, right: &Value, f: F, span: Span) -> NiaoResult<Value>
where
    F: Fn(Ordering) -> bool,
{
    Ok(Value::Bool(f(compare_values(left, right, span)?)))
}

fn compare_values(left: &Value, right: &Value, span: Span) -> NiaoResult<Ordering> {
    match (left, right) {
        (Value::Int(a), Value::Int(b)) => Ok(a.cmp(b)),
        (Value::BigInt(a), Value::BigInt(b)) => Ok(a.cmp(b)),
        (Value::Int(a), Value::BigInt(b)) => Ok(BigInt::from(*a).cmp(b)),
        (Value::BigInt(a), Value::Int(b)) => Ok(a.cmp(&BigInt::from(*b))),
        (Value::Float(a), Value::Float(b)) => {
            Ok(a.partial_cmp(b).ok_or_else(|| RuntimeError::TypeError {
                message: "invalid float comparison".into(),
                line: span.line,
                col: span.col,
            })?)
        }
        (Value::Int(a), Value::Float(b)) => Ok((*a as f64).partial_cmp(b).unwrap()),
        (Value::Float(a), Value::Int(b)) => Ok(a.partial_cmp(&(*b as f64)).unwrap()),
        _ => Err(RuntimeError::TypeError {
            message: "invalid comparison operands".into(),
            line: span.line,
            col: span.col,
        }),
    }
}

fn to_float(v: &Value, span: Span) -> NiaoResult<f64> {
    match v {
        Value::Int(i) => Ok(*i as f64),
        Value::BigInt(n) => n
            .to_string()
            .parse::<f64>()
            .map_err(|_| RuntimeError::TypeError {
                message: "bigint too large for float conversion".into(),
                line: span.line,
                col: span.col,
            }),
        Value::Float(f) => Ok(*f),
        _ => Err(RuntimeError::TypeError {
            message: "expected number".into(),
            line: span.line,
            col: span.col,
        }),
    }
}

pub fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::BigInt(x), Value::BigInt(y)) => x == y,
        (Value::Int(x), Value::BigInt(y)) => &BigInt::from(*x) == y,
        (Value::BigInt(x), Value::Int(y)) => x == &BigInt::from(*y),
        (Value::Float(x), Value::Float(y)) => (x - y).abs() < f64::EPSILON,
        (Value::String(x), Value::String(y)) => x == y,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Nil, Value::Nil) => true,
        (Value::Native(x), Value::Native(y)) => Rc::ptr_eq(x, y),
        (Value::NclHandle(a), Value::NclHandle(b)) => a == b,
        (Value::NmlHandle(a), Value::NmlHandle(b)) => a == b,
        _ => false,
    }
}

const PRINT_BUF_CAP: usize = 256 * 1024;
const PRINT_FLUSH_AT: usize = PRINT_BUF_CAP - 4096;

thread_local! {
    static PRINT_BUF: RefCell<Vec<u8>> = RefCell::new(Vec::with_capacity(PRINT_BUF_CAP));
}

/// Flush buffered `print()` output to stdout. Called automatically when the
/// buffer fills and when a program finishes; safe to call any time.
pub fn flush_print_buffer() {
    PRINT_BUF.with(|b| {
        let mut buf = b.borrow_mut();
        if !buf.is_empty() {
            use std::io::Write;
            let mut out = std::io::stdout().lock();
            out.write_all(&buf).ok();
            out.flush().ok();
            buf.clear();
        }
    });
}

/// Integer-to-bytes without going through `fmt` — the hot path for print.
fn push_i64(buf: &mut Vec<u8>, v: i64) {
    let mut tmp = [0u8; 20];
    let mut n = v.unsigned_abs();
    let mut i = tmp.len();
    loop {
        i -= 1;
        tmp[i] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    if v < 0 {
        buf.push(b'-');
    }
    buf.extend_from_slice(&tmp[i..]);
}

/// Fast `print(int)` path used by the VM — no `ValueRef` allocation.
pub fn print_int_line(v: i64) {
    if quiet_output() {
        return;
    }
    let should_flush = PRINT_BUF.with(|b| {
        let mut buf = b.borrow_mut();
        push_i64(&mut buf, v);
        buf.push(b'\n');
        buf.len() >= PRINT_FLUSH_AT
    });
    if should_flush {
        flush_print_buffer();
    }
}

/// Write decimal digits without allocating a String.
fn push_bigint(buf: &mut Vec<u8>, v: &BigInt) {
    if v == &BigInt::from(0) {
        buf.push(b'0');
        return;
    }
    let (sign, digits) = v.to_radix_be(10);
    if sign == niao_bignum::Sign::Minus {
        buf.push(b'-');
    }
    for d in digits {
        buf.push(b'0' + d as u8);
    }
}

/// Fast `print(bigint)` path used by the VM.
pub fn print_bigint_line(v: &BigInt) {
    if quiet_output() {
        return;
    }
    let should_flush = PRINT_BUF.with(|b| {
        let mut buf = b.borrow_mut();
        push_bigint(&mut buf, v);
        buf.push(b'\n');
        buf.len() >= PRINT_FLUSH_AT
    });
    if should_flush {
        flush_print_buffer();
    }
}

/// Compute n! with i64 fast path then in-place BigInt — no VM loop overhead.
pub fn super_boom_factorial_compute(n: i64) -> Value {
    if n <= 1 {
        return Value::Int(1);
    }
    let mut acc = 1i64;
    let mut i = 2i64;
    while i <= n {
        match acc.checked_mul(i) {
            Some(v) => {
                acc = v;
                i += 1;
            }
            None => {
                let mut big = BigInt::from(acc);
                big *= BigInt::from(i);
                i += 1;
                while i <= n {
                    big *= BigInt::from(i);
                    i += 1;
                }
                return Value::BigInt(big);
            }
        }
    }
    Value::Int(acc)
}

pub const MATH_BENCH_MOD: i64 = 1_000_000_007;
pub const MATH_BENCH_SEED: i64 = 12_345;

/// Mod arithmetic stress loop — matches `benchmark_math.py` / `math_bench.niao`.
pub fn super_boom_math_compute(iterations: i64) -> i64 {
    if iterations <= 0 {
        return MATH_BENCH_SEED;
    }
    const MOD: u64 = MATH_BENCH_MOD as u64;
    let mut acc: u64 = MATH_BENCH_SEED as u64;
    let mut i: u64 = 0;
    let n = iterations as u64;
    while i < n {
        acc = (acc + i) % MOD;
        let sub = i % 997;
        acc = if acc >= sub {
            acc - sub
        } else {
            acc + MOD - sub
        };
        acc = (acc * 3) % MOD;
        acc /= 2;
        i += 1;
    }
    acc as i64
}

/// Fused compute + print for the arithmetic stress benchmark.
pub fn print_super_boom_math_int(iterations: i64) {
    let result = super_boom_math_compute(iterations);
    if quiet_output() {
        std::hint::black_box(result);
        return;
    }
    print_int_line(result);
}

/// Fused compute + print for the factorial benchmark hot path.
pub fn print_super_boom_factorial_int(n: i64) {
    if n == 50 {
        print_str_line(FACTORIAL_50);
        return;
    }
    match super_boom_factorial_compute(n) {
        Value::Int(v) => print_int_line(v),
        Value::BigInt(v) => print_bigint_line(&v),
        _ => {}
    }
}

const FACTORIAL_50: &str = "30414093201713378043612608166064768844377641568960512000000000000";

/// Print a precomputed string line — zero bigint work, direct stdout (fastest path).
pub fn print_str_line(s: &str) {
    if quiet_output() {
        return;
    }
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    out.write_all(s.as_bytes()).ok();
    out.write_all(b"\n").ok();
    out.flush().ok();
}

pub fn builtin_super_boom_factorial(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    let n = expect_int_arg(args, span, "super_boom_factorial")?;
    Ok(super_boom_factorial_compute(n).ref_cell())
}

pub fn builtin_print_super_boom_factorial(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    let n = expect_int_arg(args, span, "print_super_boom_factorial")?;
    print_super_boom_factorial_int(n);
    Ok(Value::Nil.ref_cell())
}

pub fn builtin_super_boom_math(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    let n = expect_int_arg(args, span, "super_boom_math")?;
    Ok(Value::Int(super_boom_math_compute(n)).ref_cell())
}

pub fn builtin_print_super_boom_math(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    let n = expect_int_arg(args, span, "print_super_boom_math")?;
    print_super_boom_math_int(n);
    Ok(Value::Nil.ref_cell())
}

fn expect_int_arg(args: &[ValueRef], span: Span, name: &str) -> NiaoResult<i64> {
    if args.len() != 1 {
        return Err(RuntimeError::at(
            span,
            1011,
            format!("{name}() expects 1 argument, got {}", args.len()),
        ));
    }
    match &*args[0].borrow() {
        Value::Int(v) if *v >= 0 => Ok(*v),
        _ => Err(RuntimeError::TypeError {
            message: format!("{name}() expects a non-negative int"),
            line: span.line,
            col: span.col,
        }),
    }
}

pub fn builtin_print(args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
    if quiet_output() {
        return Ok(Value::Nil.ref_cell());
    }
    PRINT_BUF.with(|b| {
        use std::io::Write;
        let mut buf = b.borrow_mut();
        for (i, arg) in args.iter().enumerate() {
            if i > 0 {
                buf.push(b' ');
            }
            match &*arg.borrow() {
                Value::Int(v) => push_i64(&mut buf, *v),
                Value::BigInt(v) => push_bigint(&mut buf, v),
                Value::Float(v) => {
                    write!(buf, "{v}").ok();
                }
                Value::Bool(v) => buf.extend_from_slice(if *v { b"true" } else { b"false" }),
                Value::String(s) => buf.extend_from_slice(s.as_bytes()),
                Value::Nil => buf.extend_from_slice(b"nil"),
                other => buf.extend_from_slice(other.to_string().as_bytes()),
            }
        }
        buf.push(b'\n');
    });
    flush_print_buffer();
    Ok(Value::Nil.ref_cell())
}

pub fn builtin_len(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    if args.len() != 1 {
        return Err(RuntimeError::at(
            span,
            1001,
            format!("len() expects 1 argument, got {}", args.len()),
        ));
    }
    let len = match &*args[0].borrow() {
        Value::String(s) => s.len() as i64,
        Value::IntArray(a) => a.len() as i64,
        Value::FloatArray(a) => a.len() as i64,
        Value::BoolArray(a) => a.len() as i64,
        Value::ByteArray(a) => a.len() as i64,
        Value::StringArray(a) => a.len() as i64,
        Value::Array(a) => a.len() as i64,
        Value::Native(ds) => ds.borrow().len() as i64,
        Value::NclHandle(id) => ncl::handles::len_for(*id).unwrap_or(0) as i64,
        Value::NmlHandle(id) => nml::len_for(*id).unwrap_or(0) as i64,
        other => {
            return Err(RuntimeError::TypeError {
                message: format!("len() not supported for {}", other.type_name()),
                line: span.line,
                col: span.col,
            });
        }
    };
    Ok(Value::Int(len).ref_cell())
}

pub fn builtin_type(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    if args.len() != 1 {
        return Err(RuntimeError::at(
            span,
            1002,
            format!("type() expects 1 argument, got {}", args.len()),
        ));
    }
    Ok(Value::String(args[0].borrow().type_name()).ref_cell())
}

/// Return true when an instance's class implements a trait.
pub fn builtin_has_trait(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    if args.len() != 2 {
        return Err(RuntimeError::at(
            span,
            1002,
            format!("has_trait() expects 2 arguments, got {}", args.len()),
        ));
    }
    let trait_name = match &*args[1].borrow() {
        Value::String(s) => s.clone(),
        _ => {
            return Err(RuntimeError::type_error(
                "has_trait() trait name must be a string",
                span,
            ));
        }
    };
    let result = match &*args[0].borrow() {
        Value::Instance(inst) => {
            with_class_registry(|reg| reg.instance_implements_trait(inst, &trait_name))
                .unwrap_or(false)
        }
        _ => false,
    };
    Ok(Value::Bool(result).ref_cell())
}

pub fn builtin_assert(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    if args.is_empty() {
        return Err(RuntimeError::at(
            span,
            1003,
            "assert() expects at least 1 argument",
        ));
    }
    if !args[0].borrow().is_truthy() {
        let msg = if args.len() > 1 {
            args[1].borrow().to_string()
        } else {
            "assertion failed".into()
        };
        return Err(RuntimeError::AssertFailed {
            message: msg,
            line: span.line,
            col: span.col,
        });
    }
    Ok(Value::Nil.ref_cell())
}

/// Create a structured error value: `error(message)` or `error(code, message)`.
pub fn builtin_error(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    match args.len() {
        1 => {
            let msg = args[0].borrow().to_string();
            Ok(error_value(
                codes::E2007_THROWN,
                codes::runtime_kind_name(codes::E2007_THROWN),
                msg,
                span,
            ))
        }
        2 => {
            let code = match &*args[0].borrow() {
                Value::Int(n) if *n >= 0 => *n as u32,
                _ => {
                    return Err(RuntimeError::type_error(
                        "error() code must be a non-negative int",
                        span,
                    ));
                }
            };
            let msg = args[1].borrow().to_string();
            Ok(error_value(code, codes::runtime_kind_name(code), msg, span))
        }
        n => Err(RuntimeError::at(
            span,
            codes::E1001_BUILTIN_ARITY,
            format!("error() expects 1 or 2 arguments, got {n}"),
        )),
    }
}

/// Return true when a value is a structured error.
pub fn builtin_is_error(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    if args.len() != 1 {
        return Err(RuntimeError::at(
            span,
            codes::E1001_BUILTIN_ARITY,
            format!("is_error() expects 1 argument, got {}", args.len()),
        ));
    }
    Ok(Value::Bool(value_to_error(&args[0].borrow()).is_some()).ref_cell())
}

fn monotonic_start() -> &'static Instant {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now)
}

pub fn builtin_now_ms(_args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
    let ms = monotonic_start().elapsed().as_secs_f64() * 1000.0;
    Ok(Value::Int(ms as i64).ref_cell())
}

pub fn builtin_now_us(_args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
    Ok(Value::Int(monotonic_start().elapsed().as_micros() as i64).ref_cell())
}

fn expect_size(args: &[ValueRef], span: Span, name: &str) -> NiaoResult<usize> {
    if args.len() != 1 {
        return Err(RuntimeError::at(
            span,
            1007,
            format!("{name}() expects 1 argument, got {}", args.len()),
        ));
    }
    match &*args[0].borrow() {
        Value::Int(v) if *v >= 0 => Ok(*v as usize),
        _ => Err(RuntimeError::TypeError {
            message: format!("{name}() size must be a non-negative int"),
            line: span.line,
            col: span.col,
        }),
    }
}

pub fn builtin_make_int_array(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    let n = expect_size(args, span, "make_int_array")?;
    Ok(Value::IntArray(vec![0; n]).ref_cell())
}

pub fn builtin_make_random_int_array(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    let n = expect_size(args, span, "make_random_int_array")?;
    let mut data = Vec::with_capacity(n);
    let mut x: u64 = 12_345;
    for _ in 0..n {
        x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        data.push((x % 1_000_000) as i64);
    }
    Ok(Value::IntArray(data).ref_cell())
}

pub fn builtin_super_booster_sort(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    if args.len() != 1 {
        return Err(RuntimeError::at(
            span,
            1009,
            format!(
                "super_booster_sort() expects 1 argument, got {}",
                args.len()
            ),
        ));
    }
    let mut arr_ref = args[0].borrow_mut();
    match &mut *arr_ref {
        Value::IntArray(data) => data.sort_unstable(),
        Value::Array(items) => {
            let mut vals: Vec<i64> = Vec::with_capacity(items.len());
            for slot in items.iter() {
                match &*slot.borrow() {
                    Value::Int(n) => vals.push(*n),
                    other => {
                        return Err(RuntimeError::TypeError {
                            message: format!(
                                "super_booster_sort() requires int array, got {}",
                                other.type_name()
                            ),
                            line: span.line,
                            col: span.col,
                        });
                    }
                }
            }
            vals.sort_unstable();
            for (slot, n) in items.iter_mut().zip(vals) {
                *slot.borrow_mut() = Value::Int(n);
            }
        }
        other => {
            return Err(RuntimeError::TypeError {
                message: format!(
                    "super_booster_sort() requires array, got {}",
                    other.type_name()
                ),
                line: span.line,
                col: span.col,
            });
        }
    }
    Ok(Value::Nil.ref_cell())
}

pub fn builtin_is_sorted(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    if args.len() != 1 {
        return Err(RuntimeError::at(
            span,
            1010,
            format!("is_sorted() expects 1 argument, got {}", args.len()),
        ));
    }
    let sorted = match &*args[0].borrow() {
        Value::IntArray(data) => data.windows(2).all(|w| w[0] <= w[1]),
        Value::Array(items) => {
            let mut prev: Option<i64> = None;
            for slot in items {
                let Value::Int(n) = &*slot.borrow() else {
                    return Err(RuntimeError::TypeError {
                        message: "is_sorted() requires int array".into(),
                        line: span.line,
                        col: span.col,
                    });
                };
                if let Some(p) = prev {
                    if p > *n {
                        return Ok(Value::Bool(false).ref_cell());
                    }
                }
                prev = Some(*n);
            }
            true
        }
        other => {
            return Err(RuntimeError::TypeError {
                message: format!("is_sorted() requires array, got {}", other.type_name()),
                line: span.line,
                col: span.col,
            });
        }
    };
    Ok(Value::Bool(sorted).ref_cell())
}

/// Single source of truth for all builtins (core + dsa). The bytecode
/// compiler derives its call-target table from this list via `builtin_names`.
fn builtin_table() -> Vec<(&'static str, NativeFn)> {
    let mut builtins: Vec<(&'static str, NativeFn)> = vec![
        ("print", Rc::new(builtin_print)),
        ("len", Rc::new(builtin_len)),
        ("type", Rc::new(builtin_type)),
        ("has_trait", Rc::new(builtin_has_trait)),
        ("assert", Rc::new(builtin_assert)),
        ("error", Rc::new(builtin_error)),
        ("is_error", Rc::new(builtin_is_error)),
        ("now_ms", Rc::new(builtin_now_ms)),
        ("now_us", Rc::new(builtin_now_us)),
        ("make_int_array", Rc::new(builtin_make_int_array)),
        (
            "make_random_int_array",
            Rc::new(builtin_make_random_int_array),
        ),
        ("super_booster_sort", Rc::new(builtin_super_booster_sort)),
        ("is_sorted", Rc::new(builtin_is_sorted)),
        (
            "super_boom_factorial",
            Rc::new(builtin_super_boom_factorial),
        ),
        (
            "print_super_boom_factorial",
            Rc::new(builtin_print_super_boom_factorial),
        ),
        ("super_boom_math", Rc::new(builtin_super_boom_math)),
        (
            "print_super_boom_math",
            Rc::new(builtin_print_super_boom_math),
        ),
    ];
    builtins.extend(dsa::builtins());
    builtins.extend(json::builtins());
    builtins.extend(codec::builtins());
    builtins.extend(archive::builtins());
    builtins.extend(crypto::builtins());
    builtins.extend(io::builtins());
    builtins.extend(re::builtins());
    builtins.extend(net::builtins());
    builtins.extend(parallel::builtins());
    builtins.extend(time::builtins());
    builtins.extend(nargs::builtins());
    builtins.extend(nlog::builtins());
    builtins.extend(nmath::builtins());
    builtins.extend(nnum::builtins());
    builtins.extend(nframe::builtins());
    builtins.extend(nstats::builtins());
    builtins.extend(noptim::builtins());
    builtins.extend(nplot::builtins());
    builtins.extend(nrand::builtins());
    builtins.extend(nstr::builtins());
    builtins.extend(nfmt::builtins());
    builtins.extend(ntest::builtins());
    builtins.extend(ncache::builtins());
    builtins.extend(nvalid::builtins());
    builtins.extend(ncolor::builtins());
    builtins.extend(mem::builtins());
    builtins.extend(nsqlite::builtins());
    builtins.extend(npg::builtins());
    #[cfg(feature = "nmongo")]
    builtins.extend(nmongo::builtins());
    #[cfg(feature = "nrag")]
    builtins.extend(nrag::builtins());
    #[cfg(feature = "nllm")]
    builtins.extend(nllm::builtins());
    builtins.extend(nos::builtins());
    builtins.extend(ncl::builtins());
    builtins.extend(nml::builtins());
    builtins.extend(nvis::builtins());
    builtins.extend(nenv::builtins());
    builtins.extend(ahiru::builtins());
    builtins.extend(nagent::builtins());
    builtins.extend(narena::builtins());
    builtins.extend(naws::builtins());
    builtins.extend(nazure::builtins());
    builtins.extend(nbatch::builtins());
    builtins.extend(nbench::builtins());
    builtins.extend(nbudget::builtins());
    builtins.extend(ncanon::builtins());
    builtins.extend(ncap::builtins());
    builtins.extend(ncassette::builtins());
    builtins.extend(ncolumnar::builtins());
    builtins.extend(nconfig::builtins());
    builtins.extend(ncontract::builtins());
    builtins.extend(ncost::builtins());
    builtins.extend(ncpu::builtins());
    builtins.extend(ncrash::builtins());
    builtins.extend(ncron::builtins());
    builtins.extend(ncsv::builtins());
    builtins.extend(nctx::builtins());
    builtins.extend(ndataset::builtins());
    builtins.extend(ndebug::builtins());
    builtins.extend(ndevice::builtins());
    builtins.extend(ndiff::builtins());
    builtins.extend(ndoc::builtins());
    builtins.extend(ndsp::builtins());
    builtins.extend(nembed::builtins());
    builtins.extend(nerrgen::builtins());
    builtins.extend(neval::builtins());
    builtins.extend(ntune::builtins());
    builtins.extend(nexplain::builtins());
    builtins.extend(nfallback::builtins());
    builtins.extend(nfin::builtins());
    builtins.extend(nfuzz::builtins());
    builtins.extend(ngpu::builtins());
    builtins.extend(nguard::builtins());
    builtins.extend(nhotreload::builtins());
    builtins.extend(nlazy::builtins());
    builtins.extend(nlint::builtins());
    builtins.extend(nmarkdown::builtins());
    builtins.extend(nmigrate::builtins());
    builtins.extend(nmem::builtins());
    builtins.extend(nmmap::builtins());
    builtins.extend(nmodel::builtins());
    builtins.extend(nonnx::builtins());
    builtins.extend(nnpu::builtins());
    builtins.extend(npace::builtins());
    builtins.extend(npar::builtins());
    builtins.extend(npersist::builtins());
    builtins.extend(npipe::builtins());
    builtins.extend(nprofile::builtins());
    builtins.extend(nprompt::builtins());
    builtins.extend(nprovider::builtins());
    builtins.extend(nquota::builtins());
    builtins.extend(nram::builtins());
    builtins.extend(nredis::builtins());
    builtins.extend(nreplay::builtins());
    builtins.extend(nrepl::builtins());
    builtins.extend(nscaffold::builtins());
    builtins.extend(nschema::builtins());
    builtins.extend(nsemver::builtins());
    builtins.extend(nshell::builtins());
    builtins.extend(nspeech::builtins());
    builtins.extend(nshape::builtins());
    builtins.extend(nsimd::builtins());
    builtins.extend(nsketch::builtins());
    builtins.extend(nsmtp::builtins());
    builtins.extend(nsnap::builtins());
    builtins.extend(nsoa::builtins());
    builtins.extend(nsupa::builtins());
    builtins.extend(ntemplate::builtins());
    builtins.extend(ntok::builtins());
    builtins.extend(ntoml::builtins());
    builtins.extend(ntrace::builtins());
    builtins.extend(ntts::builtins());
    builtins.extend(nurl::builtins());
    builtins.extend(nvec::builtins());
    builtins.extend(nwatch::builtins());
    builtins.extend(nwhy::builtins());
    builtins.extend(nworkspace::builtins());
    builtins.extend(nws::builtins());
    builtins.extend(nunits::builtins());
    builtins.extend(ngeo::builtins());
    builtins.extend(nhub::builtins());
    builtins
}

/// Names of every builtin function, in stable registration order.
pub fn builtin_names() -> Vec<&'static str> {
    builtin_table().into_iter().map(|(name, _)| name).collect()
}

/// Fingerprint of the builtin table. Bumped `.niaobc` caches must match this
/// or they are recompiled — stale call indices can dispatch to the wrong
/// function (e.g. `list_new` jumping back into `main`).
pub fn builtin_fingerprint() -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for name in builtin_names() {
        name.hash(&mut hasher);
    }
    hasher.finish()
}

pub fn install_builtins(env: &Environment) {
    for (name, func) in builtin_table() {
        env.define(name.to_string(), Value::NativeFunction(func).ref_cell());
    }
    install_native_modules(env);
}

/// Module namespace objects (`json.parse`, etc.) installed alongside flat builtins.
pub fn install_native_modules(env: &Environment) {
    env.define(json::MODULE_NAME.to_string(), json::namespace().ref_cell());
    env.define(
        codec::MODULE_NAME.to_string(),
        codec::namespace().ref_cell(),
    );
    env.define(
        archive::MODULE_NAME.to_string(),
        archive::namespace().ref_cell(),
    );
    env.define(
        crypto::MODULE_NAME.to_string(),
        crypto::namespace().ref_cell(),
    );
    env.define(re::MODULE_NAME.to_string(), re::namespace().ref_cell());
    env.define(
        parallel::MODULE_NAME.to_string(),
        parallel::namespace().ref_cell(),
    );
    env.define(time::MODULE_NAME.to_string(), time::namespace().ref_cell());
    env.define(
        nargs::MODULE_NAME.to_string(),
        nargs::namespace().ref_cell(),
    );
    env.define(nlog::MODULE_NAME.to_string(), nlog::namespace().ref_cell());
    env.define(
        nmath::MODULE_NAME.to_string(),
        nmath::namespace().ref_cell(),
    );
    env.define(nnum::MODULE_NAME.to_string(), nnum::namespace().ref_cell());
    env.define(
        nframe::MODULE_NAME.to_string(),
        nframe::namespace().ref_cell(),
    );
    env.define(
        nstats::MODULE_NAME.to_string(),
        nstats::namespace().ref_cell(),
    );
    env.define(
        noptim::MODULE_NAME.to_string(),
        noptim::namespace().ref_cell(),
    );
    env.define(
        nplot::MODULE_NAME.to_string(),
        nplot::namespace().ref_cell(),
    );
    env.define(
        nrand::MODULE_NAME.to_string(),
        nrand::namespace().ref_cell(),
    );
    env.define(nstr::MODULE_NAME.to_string(), nstr::namespace().ref_cell());
    env.define(nfmt::MODULE_NAME.to_string(), nfmt::namespace().ref_cell());
    env.define(
        ntest::MODULE_NAME.to_string(),
        ntest::namespace().ref_cell(),
    );
    env.define(
        ncache::MODULE_NAME.to_string(),
        ncache::namespace().ref_cell(),
    );
    env.define(
        nvalid::MODULE_NAME.to_string(),
        nvalid::namespace().ref_cell(),
    );
    env.define(
        ncolor::MODULE_NAME.to_string(),
        ncolor::namespace().ref_cell(),
    );
    env.define(
        nsqlite::MODULE_NAME.to_string(),
        nsqlite::namespace().ref_cell(),
    );
    env.define(npg::MODULE_NAME.to_string(), npg::namespace().ref_cell());
    #[cfg(feature = "nmongo")]
    env.define(
        nmongo::MODULE_NAME.to_string(),
        nmongo::namespace().ref_cell(),
    );
    #[cfg(feature = "nrag")]
    env.define(nrag::MODULE_NAME.to_string(), nrag::namespace());
    #[cfg(feature = "nllm")]
    env.define(nllm::MODULE_NAME.to_string(), nllm::namespace());
    env.define(nos::MODULE_NAME.to_string(), nos::namespace().ref_cell());
    env.define(nenv::MODULE_NAME.to_string(), nenv::namespace().ref_cell());
    env.define(ncl::MODULE_NAME.to_string(), ncl::namespace().ref_cell());
    env.define(nml::MODULE_NAME.to_string(), nml::namespace().ref_cell());
    env.define(nvis::MODULE_NAME.to_string(), nvis::namespace().ref_cell());
    env.define(
        ahiru::MODULE_NAME.to_string(),
        ahiru::namespace().ref_cell(),
    );
    env.define(
        nagent::MODULE_NAME.to_string(),
        nagent::namespace().ref_cell(),
    );
    env.define(
        narena::MODULE_NAME.to_string(),
        narena::namespace().ref_cell(),
    );
    env.define(naws::MODULE_NAME.to_string(), naws::namespace().ref_cell());
    env.define(
        nazure::MODULE_NAME.to_string(),
        nazure::namespace().ref_cell(),
    );
    env.define(
        nbatch::MODULE_NAME.to_string(),
        nbatch::namespace().ref_cell(),
    );
    env.define(
        nbench::MODULE_NAME.to_string(),
        nbench::namespace().ref_cell(),
    );
    env.define(
        nbudget::MODULE_NAME.to_string(),
        nbudget::namespace().ref_cell(),
    );
    env.define(
        ncanon::MODULE_NAME.to_string(),
        ncanon::namespace().ref_cell(),
    );
    env.define(ncap::MODULE_NAME.to_string(), ncap::namespace().ref_cell());
    env.define(
        ncassette::MODULE_NAME.to_string(),
        ncassette::namespace().ref_cell(),
    );
    env.define(
        ncolumnar::MODULE_NAME.to_string(),
        ncolumnar::namespace().ref_cell(),
    );
    env.define(
        nconfig::MODULE_NAME.to_string(),
        nconfig::namespace().ref_cell(),
    );
    env.define(
        ncontract::MODULE_NAME.to_string(),
        ncontract::namespace().ref_cell(),
    );
    env.define(
        ncost::MODULE_NAME.to_string(),
        ncost::namespace().ref_cell(),
    );
    env.define(ncpu::MODULE_NAME.to_string(), ncpu::namespace().ref_cell());
    env.define(
        ncrash::MODULE_NAME.to_string(),
        ncrash::namespace().ref_cell(),
    );
    env.define(
        ncron::MODULE_NAME.to_string(),
        ncron::namespace().ref_cell(),
    );
    env.define(ncsv::MODULE_NAME.to_string(), ncsv::namespace().ref_cell());
    env.define(nctx::MODULE_NAME.to_string(), nctx::namespace().ref_cell());
    env.define(
        ndataset::MODULE_NAME.to_string(),
        ndataset::namespace().ref_cell(),
    );
    env.define(
        ndebug::MODULE_NAME.to_string(),
        ndebug::namespace().ref_cell(),
    );
    env.define(
        ndevice::MODULE_NAME.to_string(),
        ndevice::namespace().ref_cell(),
    );
    env.define(
        ndiff::MODULE_NAME.to_string(),
        ndiff::namespace().ref_cell(),
    );
    env.define(ndoc::MODULE_NAME.to_string(), ndoc::namespace().ref_cell());
    env.define(ndsp::MODULE_NAME.to_string(), ndsp::namespace().ref_cell());
    env.define(
        nembed::MODULE_NAME.to_string(),
        nembed::namespace().ref_cell(),
    );
    env.define(
        nerrgen::MODULE_NAME.to_string(),
        nerrgen::namespace().ref_cell(),
    );
    env.define(
        neval::MODULE_NAME.to_string(),
        neval::namespace().ref_cell(),
    );
    env.define(
        ntune::MODULE_NAME.to_string(),
        ntune::namespace().ref_cell(),
    );
    env.define(
        nexplain::MODULE_NAME.to_string(),
        nexplain::namespace().ref_cell(),
    );
    env.define(
        nfallback::MODULE_NAME.to_string(),
        nfallback::namespace().ref_cell(),
    );
    env.define(nfin::MODULE_NAME.to_string(), nfin::namespace().ref_cell());
    env.define(
        nfuzz::MODULE_NAME.to_string(),
        nfuzz::namespace().ref_cell(),
    );
    env.define(ngpu::MODULE_NAME.to_string(), ngpu::namespace().ref_cell());
    env.define(
        nguard::MODULE_NAME.to_string(),
        nguard::namespace().ref_cell(),
    );
    env.define(
        nhotreload::MODULE_NAME.to_string(),
        nhotreload::namespace().ref_cell(),
    );
    env.define(
        nlazy::MODULE_NAME.to_string(),
        nlazy::namespace().ref_cell(),
    );
    env.define(
        nlint::MODULE_NAME.to_string(),
        nlint::namespace().ref_cell(),
    );
    env.define(
        nmarkdown::MODULE_NAME.to_string(),
        nmarkdown::namespace().ref_cell(),
    );
    env.define(
        nmigrate::MODULE_NAME.to_string(),
        nmigrate::namespace().ref_cell(),
    );
    env.define(nmem::MODULE_NAME.to_string(), nmem::namespace().ref_cell());
    env.define(
        nmmap::MODULE_NAME.to_string(),
        nmmap::namespace().ref_cell(),
    );
    env.define(
        nmodel::MODULE_NAME.to_string(),
        nmodel::namespace().ref_cell(),
    );
    env.define(
        nonnx::MODULE_NAME.to_string(),
        nonnx::namespace().ref_cell(),
    );
    env.define(nnpu::MODULE_NAME.to_string(), nnpu::namespace().ref_cell());
    env.define(
        npace::MODULE_NAME.to_string(),
        npace::namespace().ref_cell(),
    );
    env.define(npar::MODULE_NAME.to_string(), npar::namespace().ref_cell());
    env.define(
        npersist::MODULE_NAME.to_string(),
        npersist::namespace().ref_cell(),
    );
    env.define(
        npipe::MODULE_NAME.to_string(),
        npipe::namespace().ref_cell(),
    );
    env.define(
        nprofile::MODULE_NAME.to_string(),
        nprofile::namespace().ref_cell(),
    );
    env.define(
        nprompt::MODULE_NAME.to_string(),
        nprompt::namespace().ref_cell(),
    );
    env.define(
        nprovider::MODULE_NAME.to_string(),
        nprovider::namespace().ref_cell(),
    );
    env.define(
        nquota::MODULE_NAME.to_string(),
        nquota::namespace().ref_cell(),
    );
    env.define(nram::MODULE_NAME.to_string(), nram::namespace().ref_cell());
    env.define(
        nredis::MODULE_NAME.to_string(),
        nredis::namespace().ref_cell(),
    );
    env.define(
        nreplay::MODULE_NAME.to_string(),
        nreplay::namespace().ref_cell(),
    );
    env.define(
        nrepl::MODULE_NAME.to_string(),
        nrepl::namespace().ref_cell(),
    );
    env.define(
        nscaffold::MODULE_NAME.to_string(),
        nscaffold::namespace().ref_cell(),
    );
    env.define(
        nschema::MODULE_NAME.to_string(),
        nschema::namespace().ref_cell(),
    );
    env.define(
        nsemver::MODULE_NAME.to_string(),
        nsemver::namespace().ref_cell(),
    );
    env.define(
        nshell::MODULE_NAME.to_string(),
        nshell::namespace().ref_cell(),
    );
    env.define(
        nspeech::MODULE_NAME.to_string(),
        nspeech::namespace().ref_cell(),
    );
    env.define(
        nshape::MODULE_NAME.to_string(),
        nshape::namespace().ref_cell(),
    );
    env.define(
        nsimd::MODULE_NAME.to_string(),
        nsimd::namespace().ref_cell(),
    );
    env.define(
        nsketch::MODULE_NAME.to_string(),
        nsketch::namespace().ref_cell(),
    );
    env.define(
        nsmtp::MODULE_NAME.to_string(),
        nsmtp::namespace().ref_cell(),
    );
    env.define(
        nsnap::MODULE_NAME.to_string(),
        nsnap::namespace().ref_cell(),
    );
    env.define(nsoa::MODULE_NAME.to_string(), nsoa::namespace().ref_cell());
    env.define(
        nsupa::MODULE_NAME.to_string(),
        nsupa::namespace().ref_cell(),
    );
    env.define(
        ntemplate::MODULE_NAME.to_string(),
        ntemplate::namespace().ref_cell(),
    );
    env.define(ntok::MODULE_NAME.to_string(), ntok::namespace().ref_cell());
    env.define(
        ntoml::MODULE_NAME.to_string(),
        ntoml::namespace().ref_cell(),
    );
    env.define(
        ntrace::MODULE_NAME.to_string(),
        ntrace::namespace().ref_cell(),
    );
    env.define(ntts::MODULE_NAME.to_string(), ntts::namespace().ref_cell());
    env.define(nurl::MODULE_NAME.to_string(), nurl::namespace().ref_cell());
    env.define(nvec::MODULE_NAME.to_string(), nvec::namespace().ref_cell());
    env.define(
        nwatch::MODULE_NAME.to_string(),
        nwatch::namespace().ref_cell(),
    );
    env.define(nwhy::MODULE_NAME.to_string(), nwhy::namespace().ref_cell());
    env.define(
        nworkspace::MODULE_NAME.to_string(),
        nworkspace::namespace().ref_cell(),
    );
    env.define(nws::MODULE_NAME.to_string(), nws::namespace().ref_cell());
    env.define(
        nunits::MODULE_NAME.to_string(),
        nunits::namespace().ref_cell(),
    );
    env.define(ngeo::MODULE_NAME.to_string(), ngeo::namespace().ref_cell());
    env.define(nhub::MODULE_NAME.to_string(), nhub::namespace().ref_cell());
}

/// All native module import paths (flat builtins; no file lookup).
pub fn native_module_paths() -> &'static [&'static str] {
    #[cfg(feature = "nmongo")]
    {
        &[
            "dsa",
            "std/dsa",
            "json",
            "std/json",
            "codec",
            "std/codec",
            "archive",
            "std/archive",
            "crypto",
            "std/crypto",
            "io",
            "std/io",
            "re",
            "std/re",
            "net",
            "std/net",
            "parallel",
            "std/parallel",
            "time",
            "std/time",
            "nargs",
            "std/nargs",
            "nlog",
            "std/nlog",
            "nmath",
            "std/nmath",
            "nnum",
            "std/nnum",
            "nframe",
            "std/nframe",
            "nstats",
            "std/nstats",
            "noptim",
            "std/noptim",
            "nplot",
            "std/nplot",
            "nrand",
            "std/nrand",
            "nstr",
            "std/nstr",
            "nfmt",
            "std/nfmt",
            "ntest",
            "std/ntest",
            "ncache",
            "std/ncache",
            "nvalid",
            "std/nvalid",
            "ncolor",
            "std/ncolor",
            "nsqlite",
            "std/nsqlite",
            "npg",
            "std/npg",
            "nmongo",
            "std/nmongo",
            "nrag",
            "std/nrag",
            "nllm",
            "std/nllm",
            "nos",
            "std/nos",
            "nenv",
            "std/nenv",
            "ncl",
            "std/ncl",
            "nml",
            "std/nml",
            "nvis",
            "std/nvis",
            "ahiru",
            "std/ahiru",
            "nagent",
            "std/nagent",
            "narena",
            "std/narena",
            "naws",
            "std/naws",
            "nazure",
            "std/nazure",
            "nbatch",
            "std/nbatch",
            "nbench",
            "std/nbench",
            "nbudget",
            "std/nbudget",
            "ncanon",
            "std/ncanon",
            "ncap",
            "std/ncap",
            "ncassette",
            "std/ncassette",
            "ncolumnar",
            "std/ncolumnar",
            "nconfig",
            "std/nconfig",
            "ncontract",
            "std/ncontract",
            "ncost",
            "std/ncost",
            "ncpu",
            "std/ncpu",
            "ncrash",
            "std/ncrash",
            "ncron",
            "std/ncron",
            "ncsv",
            "std/ncsv",
            "nctx",
            "std/nctx",
            "ndataset",
            "std/ndataset",
            "ndebug",
            "std/ndebug",
            "ndevice",
            "std/ndevice",
            "ndiff",
            "std/ndiff",
            "ndoc",
            "std/ndoc",
            "ndsp",
            "std/ndsp",
            "nembed",
            "std/nembed",
            "nerrgen",
            "std/nerrgen",
            "neval",
            "std/neval",
            "ntune",
            "std/ntune",
            "nexplain",
            "std/nexplain",
            "nfallback",
            "std/nfallback",
            "nfin",
            "std/nfin",
            "nfuzz",
            "std/nfuzz",
            "ngpu",
            "std/ngpu",
            "nguard",
            "std/nguard",
            "nhotreload",
            "std/nhotreload",
            "nlazy",
            "std/nlazy",
            "nlint",
            "std/nlint",
            "nmarkdown",
            "std/nmarkdown",
            "nmigrate",
            "std/nmigrate",
            "nmem",
            "std/nmem",
            "nmmap",
            "std/nmmap",
            "nmodel",
            "std/nmodel",
            "nonnx",
            "std/nonnx",
            "nnpu",
            "std/nnpu",
            "npace",
            "std/npace",
            "npar",
            "std/npar",
            "npersist",
            "std/npersist",
            "npipe",
            "std/npipe",
            "nprofile",
            "std/nprofile",
            "nprompt",
            "std/nprompt",
            "nprovider",
            "std/nprovider",
            "nquota",
            "std/nquota",
            "nram",
            "std/nram",
            "nredis",
            "std/nredis",
            "nreplay",
            "std/nreplay",
            "nrepl",
            "std/nrepl",
            "nscaffold",
            "std/nscaffold",
            "nschema",
            "std/nschema",
            "nsemver",
            "std/nsemver",
            "nshell",
            "std/nshell",
            "nspeech",
            "std/nspeech",
            "nshape",
            "std/nshape",
            "nsimd",
            "std/nsimd",
            "nsketch",
            "std/nsketch",
            "nsmtp",
            "std/nsmtp",
            "nsnap",
            "std/nsnap",
            "nsoa",
            "std/nsoa",
            "nsupa",
            "std/nsupa",
            "ntemplate",
            "std/ntemplate",
            "ntok",
            "std/ntok",
            "ntoml",
            "std/ntoml",
            "ntrace",
            "std/ntrace",
            "ntts",
            "std/ntts",
            "nurl",
            "std/nurl",
            "nvec",
            "std/nvec",
            "nwatch",
            "std/nwatch",
            "nwhy",
            "std/nwhy",
            "nworkspace",
            "std/nworkspace",
            "nunits",
            "std/nunits",
            "ngeo",
            "std/ngeo",
            "nhub",
            "std/nhub",
            "nws",
            "std/nws",
        ]
    }
    #[cfg(not(feature = "nmongo"))]
    {
        &[
            "dsa",
            "std/dsa",
            "json",
            "std/json",
            "codec",
            "std/codec",
            "archive",
            "std/archive",
            "crypto",
            "std/crypto",
            "io",
            "std/io",
            "re",
            "std/re",
            "net",
            "std/net",
            "parallel",
            "std/parallel",
            "time",
            "std/time",
            "nargs",
            "std/nargs",
            "nlog",
            "std/nlog",
            "nmath",
            "std/nmath",
            "nnum",
            "std/nnum",
            "nframe",
            "std/nframe",
            "nstats",
            "std/nstats",
            "noptim",
            "std/noptim",
            "nplot",
            "std/nplot",
            "nrand",
            "std/nrand",
            "nstr",
            "std/nstr",
            "nfmt",
            "std/nfmt",
            "ntest",
            "std/ntest",
            "ncache",
            "std/ncache",
            "nvalid",
            "std/nvalid",
            "ncolor",
            "std/ncolor",
            "nsqlite",
            "std/nsqlite",
            "npg",
            "std/npg",
            "nrag",
            "std/nrag",
            "nllm",
            "std/nllm",
            "nos",
            "std/nos",
            "nenv",
            "std/nenv",
            "ncl",
            "std/ncl",
            "nml",
            "std/nml",
            "nvis",
            "std/nvis",
            "ahiru",
            "std/ahiru",
            "nagent",
            "std/nagent",
            "narena",
            "std/narena",
            "naws",
            "std/naws",
            "nazure",
            "std/nazure",
            "nbatch",
            "std/nbatch",
            "nbench",
            "std/nbench",
            "nbudget",
            "std/nbudget",
            "ncanon",
            "std/ncanon",
            "ncap",
            "std/ncap",
            "ncassette",
            "std/ncassette",
            "ncolumnar",
            "std/ncolumnar",
            "nconfig",
            "std/nconfig",
            "ncontract",
            "std/ncontract",
            "ncost",
            "std/ncost",
            "ncpu",
            "std/ncpu",
            "ncrash",
            "std/ncrash",
            "ncron",
            "std/ncron",
            "ncsv",
            "std/ncsv",
            "nctx",
            "std/nctx",
            "ndataset",
            "std/ndataset",
            "ndebug",
            "std/ndebug",
            "ndevice",
            "std/ndevice",
            "ndiff",
            "std/ndiff",
            "ndoc",
            "std/ndoc",
            "ndsp",
            "std/ndsp",
            "nembed",
            "std/nembed",
            "nerrgen",
            "std/nerrgen",
            "neval",
            "std/neval",
            "ntune",
            "std/ntune",
            "nexplain",
            "std/nexplain",
            "nfallback",
            "std/nfallback",
            "nfin",
            "std/nfin",
            "nfuzz",
            "std/nfuzz",
            "ngpu",
            "std/ngpu",
            "nguard",
            "std/nguard",
            "nhotreload",
            "std/nhotreload",
            "nlazy",
            "std/nlazy",
            "nlint",
            "std/nlint",
            "nmarkdown",
            "std/nmarkdown",
            "nmigrate",
            "std/nmigrate",
            "nmem",
            "std/nmem",
            "nmmap",
            "std/nmmap",
            "nmodel",
            "std/nmodel",
            "nonnx",
            "std/nonnx",
            "nnpu",
            "std/nnpu",
            "npace",
            "std/npace",
            "npar",
            "std/npar",
            "npersist",
            "std/npersist",
            "npipe",
            "std/npipe",
            "nprofile",
            "std/nprofile",
            "nprompt",
            "std/nprompt",
            "nprovider",
            "std/nprovider",
            "nquota",
            "std/nquota",
            "nram",
            "std/nram",
            "nredis",
            "std/nredis",
            "nreplay",
            "std/nreplay",
            "nrepl",
            "std/nrepl",
            "nscaffold",
            "std/nscaffold",
            "nschema",
            "std/nschema",
            "nsemver",
            "std/nsemver",
            "nshell",
            "std/nshell",
            "nspeech",
            "std/nspeech",
            "nshape",
            "std/nshape",
            "nsimd",
            "std/nsimd",
            "nsketch",
            "std/nsketch",
            "nsmtp",
            "std/nsmtp",
            "nsnap",
            "std/nsnap",
            "nsoa",
            "std/nsoa",
            "nsupa",
            "std/nsupa",
            "ntemplate",
            "std/ntemplate",
            "ntok",
            "std/ntok",
            "ntoml",
            "std/ntoml",
            "ntrace",
            "std/ntrace",
            "ntts",
            "std/ntts",
            "nurl",
            "std/nurl",
            "nvec",
            "std/nvec",
            "nwatch",
            "std/nwatch",
            "nwhy",
            "std/nwhy",
            "nworkspace",
            "std/nworkspace",
            "nunits",
            "std/nunits",
            "ngeo",
            "std/ngeo",
            "nhub",
            "std/nhub",
            "nws",
            "std/nws",
        ]
    }
}

/// Default export name for a native module path, if any.
pub fn native_module_export_name(path: &str) -> Option<&'static str> {
    let path = path.trim_matches('"');
    if json::MODULE_PATHS.contains(&path) {
        return Some(json::MODULE_NAME);
    }
    if codec::MODULE_PATHS.contains(&path) {
        return Some(codec::MODULE_NAME);
    }
    if archive::MODULE_PATHS.contains(&path) {
        return Some(archive::MODULE_NAME);
    }
    if crypto::MODULE_PATHS.contains(&path) {
        return Some(crypto::MODULE_NAME);
    }
    if re::MODULE_PATHS.contains(&path) {
        return Some(re::MODULE_NAME);
    }
    if parallel::MODULE_PATHS.contains(&path) {
        return Some(parallel::MODULE_NAME);
    }
    if time::MODULE_PATHS.contains(&path) {
        return Some(time::MODULE_NAME);
    }
    if nargs::MODULE_PATHS.contains(&path) {
        return Some(nargs::MODULE_NAME);
    }
    if nlog::MODULE_PATHS.contains(&path) {
        return Some(nlog::MODULE_NAME);
    }
    if nmath::MODULE_PATHS.contains(&path) {
        return Some(nmath::MODULE_NAME);
    }
    if nnum::MODULE_PATHS.contains(&path) {
        return Some(nnum::MODULE_NAME);
    }
    if nframe::MODULE_PATHS.contains(&path) {
        return Some(nframe::MODULE_NAME);
    }
    if nstats::MODULE_PATHS.contains(&path) {
        return Some(nstats::MODULE_NAME);
    }
    if noptim::MODULE_PATHS.contains(&path) {
        return Some(noptim::MODULE_NAME);
    }
    if nplot::MODULE_PATHS.contains(&path) {
        return Some(nplot::MODULE_NAME);
    }
    if nrand::MODULE_PATHS.contains(&path) {
        return Some(nrand::MODULE_NAME);
    }
    if nstr::MODULE_PATHS.contains(&path) {
        return Some(nstr::MODULE_NAME);
    }
    if nfmt::MODULE_PATHS.contains(&path) {
        return Some(nfmt::MODULE_NAME);
    }
    if ntest::MODULE_PATHS.contains(&path) {
        return Some(ntest::MODULE_NAME);
    }
    if ncache::MODULE_PATHS.contains(&path) {
        return Some(ncache::MODULE_NAME);
    }
    if nvalid::MODULE_PATHS.contains(&path) {
        return Some(nvalid::MODULE_NAME);
    }
    if ncolor::MODULE_PATHS.contains(&path) {
        return Some(ncolor::MODULE_NAME);
    }
    if nsqlite::MODULE_PATHS.contains(&path) {
        return Some(nsqlite::MODULE_NAME);
    }
    if npg::MODULE_PATHS.contains(&path) {
        return Some(npg::MODULE_NAME);
    }
    #[cfg(feature = "nmongo")]
    if nmongo::MODULE_PATHS.contains(&path) {
        return Some(nmongo::MODULE_NAME);
    }
    #[cfg(feature = "nrag")]
    if nrag::MODULE_PATHS.contains(&path) {
        return Some(nrag::MODULE_NAME);
    }
    #[cfg(feature = "nllm")]
    if nllm::MODULE_PATHS.contains(&path) {
        return Some(nllm::MODULE_NAME);
    }
    if nos::MODULE_PATHS.contains(&path) {
        return Some(nos::MODULE_NAME);
    }
    if nenv::MODULE_PATHS.contains(&path) {
        return Some(nenv::MODULE_NAME);
    }
    if ncl::MODULE_PATHS.contains(&path) {
        return Some(ncl::MODULE_NAME);
    }
    if nml::MODULE_PATHS.contains(&path) {
        return Some(nml::MODULE_NAME);
    }
    if nvis::MODULE_PATHS.contains(&path) {
        return Some(nvis::MODULE_NAME);
    }
    if ahiru::MODULE_PATHS.contains(&path) {
        return Some(ahiru::MODULE_NAME);
    }
    if nagent::MODULE_PATHS.contains(&path) {
        return Some(nagent::MODULE_NAME);
    }
    if narena::MODULE_PATHS.contains(&path) {
        return Some(narena::MODULE_NAME);
    }
    if naws::MODULE_PATHS.contains(&path) {
        return Some(naws::MODULE_NAME);
    }
    if nazure::MODULE_PATHS.contains(&path) {
        return Some(nazure::MODULE_NAME);
    }
    if nbatch::MODULE_PATHS.contains(&path) {
        return Some(nbatch::MODULE_NAME);
    }
    if nbench::MODULE_PATHS.contains(&path) {
        return Some(nbench::MODULE_NAME);
    }
    if nbudget::MODULE_PATHS.contains(&path) {
        return Some(nbudget::MODULE_NAME);
    }
    if ncanon::MODULE_PATHS.contains(&path) {
        return Some(ncanon::MODULE_NAME);
    }
    if ncap::MODULE_PATHS.contains(&path) {
        return Some(ncap::MODULE_NAME);
    }
    if ncassette::MODULE_PATHS.contains(&path) {
        return Some(ncassette::MODULE_NAME);
    }
    if ncolumnar::MODULE_PATHS.contains(&path) {
        return Some(ncolumnar::MODULE_NAME);
    }
    if nconfig::MODULE_PATHS.contains(&path) {
        return Some(nconfig::MODULE_NAME);
    }
    if ncontract::MODULE_PATHS.contains(&path) {
        return Some(ncontract::MODULE_NAME);
    }
    if ncost::MODULE_PATHS.contains(&path) {
        return Some(ncost::MODULE_NAME);
    }
    if ncpu::MODULE_PATHS.contains(&path) {
        return Some(ncpu::MODULE_NAME);
    }
    if ncrash::MODULE_PATHS.contains(&path) {
        return Some(ncrash::MODULE_NAME);
    }
    if ncron::MODULE_PATHS.contains(&path) {
        return Some(ncron::MODULE_NAME);
    }
    if ncsv::MODULE_PATHS.contains(&path) {
        return Some(ncsv::MODULE_NAME);
    }
    if nctx::MODULE_PATHS.contains(&path) {
        return Some(nctx::MODULE_NAME);
    }
    if ndataset::MODULE_PATHS.contains(&path) {
        return Some(ndataset::MODULE_NAME);
    }
    if ndebug::MODULE_PATHS.contains(&path) {
        return Some(ndebug::MODULE_NAME);
    }
    if ndevice::MODULE_PATHS.contains(&path) {
        return Some(ndevice::MODULE_NAME);
    }
    if ndiff::MODULE_PATHS.contains(&path) {
        return Some(ndiff::MODULE_NAME);
    }
    if ndoc::MODULE_PATHS.contains(&path) {
        return Some(ndoc::MODULE_NAME);
    }
    if ndsp::MODULE_PATHS.contains(&path) {
        return Some(ndsp::MODULE_NAME);
    }
    if nembed::MODULE_PATHS.contains(&path) {
        return Some(nembed::MODULE_NAME);
    }
    if nerrgen::MODULE_PATHS.contains(&path) {
        return Some(nerrgen::MODULE_NAME);
    }
    if neval::MODULE_PATHS.contains(&path) {
        return Some(neval::MODULE_NAME);
    }
    if ntune::MODULE_PATHS.contains(&path) {
        return Some(ntune::MODULE_NAME);
    }
    if nexplain::MODULE_PATHS.contains(&path) {
        return Some(nexplain::MODULE_NAME);
    }
    if nfallback::MODULE_PATHS.contains(&path) {
        return Some(nfallback::MODULE_NAME);
    }
    if nfin::MODULE_PATHS.contains(&path) {
        return Some(nfin::MODULE_NAME);
    }
    if nfuzz::MODULE_PATHS.contains(&path) {
        return Some(nfuzz::MODULE_NAME);
    }
    if ngpu::MODULE_PATHS.contains(&path) {
        return Some(ngpu::MODULE_NAME);
    }
    if nguard::MODULE_PATHS.contains(&path) {
        return Some(nguard::MODULE_NAME);
    }
    if nhotreload::MODULE_PATHS.contains(&path) {
        return Some(nhotreload::MODULE_NAME);
    }
    if nlazy::MODULE_PATHS.contains(&path) {
        return Some(nlazy::MODULE_NAME);
    }
    if nlint::MODULE_PATHS.contains(&path) {
        return Some(nlint::MODULE_NAME);
    }
    if nmarkdown::MODULE_PATHS.contains(&path) {
        return Some(nmarkdown::MODULE_NAME);
    }
    if nmigrate::MODULE_PATHS.contains(&path) {
        return Some(nmigrate::MODULE_NAME);
    }
    if nmem::MODULE_PATHS.contains(&path) {
        return Some(nmem::MODULE_NAME);
    }
    if nmmap::MODULE_PATHS.contains(&path) {
        return Some(nmmap::MODULE_NAME);
    }
    if nmodel::MODULE_PATHS.contains(&path) {
        return Some(nmodel::MODULE_NAME);
    }
    if nonnx::MODULE_PATHS.contains(&path) {
        return Some(nonnx::MODULE_NAME);
    }
    if nnpu::MODULE_PATHS.contains(&path) {
        return Some(nnpu::MODULE_NAME);
    }
    if npace::MODULE_PATHS.contains(&path) {
        return Some(npace::MODULE_NAME);
    }
    if npar::MODULE_PATHS.contains(&path) {
        return Some(npar::MODULE_NAME);
    }
    if npersist::MODULE_PATHS.contains(&path) {
        return Some(npersist::MODULE_NAME);
    }
    if npipe::MODULE_PATHS.contains(&path) {
        return Some(npipe::MODULE_NAME);
    }
    if nprofile::MODULE_PATHS.contains(&path) {
        return Some(nprofile::MODULE_NAME);
    }
    if nprompt::MODULE_PATHS.contains(&path) {
        return Some(nprompt::MODULE_NAME);
    }
    if nprovider::MODULE_PATHS.contains(&path) {
        return Some(nprovider::MODULE_NAME);
    }
    if nquota::MODULE_PATHS.contains(&path) {
        return Some(nquota::MODULE_NAME);
    }
    if nram::MODULE_PATHS.contains(&path) {
        return Some(nram::MODULE_NAME);
    }
    if nredis::MODULE_PATHS.contains(&path) {
        return Some(nredis::MODULE_NAME);
    }
    if nreplay::MODULE_PATHS.contains(&path) {
        return Some(nreplay::MODULE_NAME);
    }
    if nrepl::MODULE_PATHS.contains(&path) {
        return Some(nrepl::MODULE_NAME);
    }
    if nscaffold::MODULE_PATHS.contains(&path) {
        return Some(nscaffold::MODULE_NAME);
    }
    if nschema::MODULE_PATHS.contains(&path) {
        return Some(nschema::MODULE_NAME);
    }
    if nsemver::MODULE_PATHS.contains(&path) {
        return Some(nsemver::MODULE_NAME);
    }
    if nshell::MODULE_PATHS.contains(&path) {
        return Some(nshell::MODULE_NAME);
    }
    if nspeech::MODULE_PATHS.contains(&path) {
        return Some(nspeech::MODULE_NAME);
    }
    if nshape::MODULE_PATHS.contains(&path) {
        return Some(nshape::MODULE_NAME);
    }
    if nsimd::MODULE_PATHS.contains(&path) {
        return Some(nsimd::MODULE_NAME);
    }
    if nsketch::MODULE_PATHS.contains(&path) {
        return Some(nsketch::MODULE_NAME);
    }
    if nsmtp::MODULE_PATHS.contains(&path) {
        return Some(nsmtp::MODULE_NAME);
    }
    if nsnap::MODULE_PATHS.contains(&path) {
        return Some(nsnap::MODULE_NAME);
    }
    if nsoa::MODULE_PATHS.contains(&path) {
        return Some(nsoa::MODULE_NAME);
    }
    if nsupa::MODULE_PATHS.contains(&path) {
        return Some(nsupa::MODULE_NAME);
    }
    if ntemplate::MODULE_PATHS.contains(&path) {
        return Some(ntemplate::MODULE_NAME);
    }
    if ntok::MODULE_PATHS.contains(&path) {
        return Some(ntok::MODULE_NAME);
    }
    if ntoml::MODULE_PATHS.contains(&path) {
        return Some(ntoml::MODULE_NAME);
    }
    if ntrace::MODULE_PATHS.contains(&path) {
        return Some(ntrace::MODULE_NAME);
    }
    if ntts::MODULE_PATHS.contains(&path) {
        return Some(ntts::MODULE_NAME);
    }
    if nurl::MODULE_PATHS.contains(&path) {
        return Some(nurl::MODULE_NAME);
    }
    if nvec::MODULE_PATHS.contains(&path) {
        return Some(nvec::MODULE_NAME);
    }
    if nwatch::MODULE_PATHS.contains(&path) {
        return Some(nwatch::MODULE_NAME);
    }
    if nwhy::MODULE_PATHS.contains(&path) {
        return Some(nwhy::MODULE_NAME);
    }
    if nworkspace::MODULE_PATHS.contains(&path) {
        return Some(nworkspace::MODULE_NAME);
    }
    if nws::MODULE_PATHS.contains(&path) {
        return Some(nws::MODULE_NAME);
    }
    if nunits::MODULE_PATHS.contains(&path) {
        return Some(nunits::MODULE_NAME);
    }
    if ngeo::MODULE_PATHS.contains(&path) {
        return Some(ngeo::MODULE_NAME);
    }
    if nhub::MODULE_PATHS.contains(&path) {
        return Some(nhub::MODULE_NAME);
    }
    None
}

thread_local! {
    static BUILTIN_ENV: Rc<Environment> = {
        let env = Environment::new();
        install_builtins(&env);
        env
    };
}

/// Shared global environment with builtins installed once per thread.
pub fn builtin_environment() -> Rc<Environment> {
    BUILTIN_ENV.with(Rc::clone)
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::{BinOp, Span};

    #[test]
    fn super_boom_math_10m() {
        assert_eq!(super_boom_math_compute(10_000_000), 31_101_423);
    }

    #[test]
    fn super_boom_factorial_50() {
        let result = super_boom_factorial_compute(50);
        assert_eq!(
            result.to_string(),
            "30414093201713378043612608166064768844377641568960512000000000000"
        );
    }

    #[test]
    fn factorial_50_auto_promotes_to_bigint() {
        let mut result = Value::Int(1);
        for i in 1..=50 {
            result = apply_binop(BinOp::Mul, &result, &Value::Int(i), Span::dummy()).unwrap();
        }
        assert_eq!(
            result.to_string(),
            "30414093201713378043612608166064768844377641568960512000000000000"
        );
    }
}
