//! Runtime values. Intentionally small — HScript is dynamically typed.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

use rayzor_parser::haxe_ast::Function;

/// A reference-counted dynamic object. Used for Haxe object literals and
/// anything a host exposes as a "struct-like" value.
pub type ObjectRef = Rc<RefCell<HashMap<String, Value>>>;

/// A reference-counted growable array.
pub type ArrayRef = Rc<RefCell<Vec<Value>>>;

/// A closure captured at definition time. Holds the function AST plus a
/// snapshot of the scope it closes over.
#[derive(Clone)]
pub struct Closure {
    pub func: Function,
    pub captured: crate::scope::Scope,
}

impl fmt::Debug for Closure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Closure")
            .field("name", &self.func.name)
            .field("params", &self.func.params.len())
            .finish()
    }
}

/// A host-provided function. The `Rc` lets the interpreter keep it alive after
/// the host releases it — important because scripts can stash callbacks in
/// vars.
pub type HostFn = Rc<dyn Fn(&[Value]) -> Result<Value, String>>;

#[derive(Clone)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(Rc<String>),
    Array(ArrayRef),
    Object(ObjectRef),
    Closure(Rc<Closure>),
    HostFn(HostFn),
    /// Opaque handle into the host (e.g. a sprite id). The string tag is a
    /// coarse type marker so the interpreter can distinguish `"sprite"` from
    /// `"tween"` without inspecting the numeric id.
    Handle { tag: &'static str, id: u64 },
}

impl Default for Value {
    fn default() -> Self {
        Value::Null
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Null => write!(f, "null"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Int(i) => write!(f, "{i}"),
            Value::Float(x) => write!(f, "{x}"),
            Value::String(s) => write!(f, "{:?}", s.as_str()),
            Value::Array(a) => f.debug_list().entries(a.borrow().iter()).finish(),
            Value::Object(o) => f.debug_map().entries(o.borrow().iter()).finish(),
            Value::Closure(c) => write!(f, "<closure {}>", c.func.name),
            Value::HostFn(_) => write!(f, "<host fn>"),
            Value::Handle { tag, id } => write!(f, "<{tag} #{id}>"),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Null => write!(f, "null"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Int(i) => write!(f, "{i}"),
            Value::Float(x) => write!(f, "{x}"),
            Value::String(s) => write!(f, "{}", s.as_str()),
            other => write!(f, "{other:?}"),
        }
    }
}

impl Value {
    pub fn from_str(s: impl Into<String>) -> Self {
        Value::String(Rc::new(s.into()))
    }

    pub fn new_array(vs: Vec<Value>) -> Self {
        Value::Array(Rc::new(RefCell::new(vs)))
    }

    pub fn new_object() -> Self {
        Value::Object(Rc::new(RefCell::new(HashMap::new())))
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Null => false,
            Value::Bool(b) => *b,
            Value::Int(i) => *i != 0,
            Value::Float(f) => *f != 0.0 && !f.is_nan(),
            Value::String(s) => !s.is_empty(),
            _ => true,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Int(i) => Some(*i as f64),
            Value::Float(f) => Some(*f),
            Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            Value::Float(f) => Some(*f as i64),
            Value::Bool(b) => Some(if *b { 1 } else { 0 }),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Loose equality — matches Haxe's `==` closely enough for HScript use:
    /// numeric types compare by value, strings by content, everything else by
    /// reference/identity.
    pub fn loose_eq(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Null, Value::Null) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Int(a), Value::Float(b)) | (Value::Float(b), Value::Int(a)) => {
                (*a as f64) == *b
            }
            (Value::Array(a), Value::Array(b)) => Rc::ptr_eq(a, b),
            (Value::Object(a), Value::Object(b)) => Rc::ptr_eq(a, b),
            (Value::Closure(a), Value::Closure(b)) => Rc::ptr_eq(a, b),
            (
                Value::Handle { tag: t1, id: i1 },
                Value::Handle { tag: t2, id: i2 },
            ) => t1 == t2 && i1 == i2,
            _ => false,
        }
    }
}
