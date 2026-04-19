//! Bridge between HScript and the hosting engine (rustic-scripting /
//! rustic-gameplay). The interpreter itself knows nothing about sprites,
//! tweens, or Psych callbacks — all of that goes through [`HostBridge`].
//!
//! Keeping this trait here (not in rustic-scripting) lets us unit-test the
//! interpreter with a no-op host.

use crate::value::Value;

/// Interface the embedder implements. Methods are fallible so scripts can
/// trigger runtime errors gracefully (missing property, wrong type, etc.).
///
/// Every method receives a plain string key so the host can use any backing
/// store (hash map, ECS, whatever) without coupling to our Value shape.
pub trait HostBridge {
    /// Look up a name in the host's global environment. Called when HScript
    /// evaluates an identifier that isn't a local binding. Returns
    /// `Ok(Value::Null)` if the host simply doesn't know the name, or an error
    /// for a "forbidden" access.
    fn global_get(&mut self, _name: &str) -> Result<Value, String> {
        Ok(Value::Null)
    }

    /// Try to set a host global. Return `Ok(true)` if the host claimed the
    /// name, `Ok(false)` to let the interpreter fall back to script-side
    /// globals. Default: defer to the interpreter.
    fn global_set(&mut self, _name: &str, _value: &Value) -> Result<bool, String> {
        Ok(false)
    }

    /// Invoke a host-provided global function. Returning `Ok(None)` lets the
    /// interpreter continue normal lookup and produce its usual callable error.
    fn global_call(&mut self, _name: &str, _args: &[Value]) -> Result<Option<Value>, String> {
        Ok(None)
    }

    /// Read a property/field from a host-owned value (typically a
    /// [`Value::Handle`] — e.g. a sprite id).
    fn field_get(&mut self, _target: &Value, field: &str) -> Result<Value, String> {
        Err(format!("field '{field}' not available on this value"))
    }

    /// Write a property/field on a host-owned value.
    fn field_set(&mut self, _target: &Value, field: &str, _value: &Value) -> Result<(), String> {
        Err(format!("cannot set field '{field}' on this value"))
    }

    /// Invoke a host-owned callable. For [`Value::HostFn`] the interpreter
    /// calls the closure directly; this method handles method calls on
    /// handles (e.g. `sprite.playAnim("idle")`).
    fn method_call(
        &mut self,
        _target: &Value,
        method: &str,
        _args: &[Value],
    ) -> Result<Value, String> {
        Err(format!("method '{method}' not available on this value"))
    }

    /// Construct a host type by name: `new FlxSprite()` → handle.
    fn construct(&mut self, type_name: &str, _args: &[Value]) -> Result<Value, String> {
        Err(format!("unknown type '{type_name}'"))
    }
}

/// Trivial host that knows nothing. Used by tests and as a placeholder.
#[derive(Default)]
pub struct NoopHost;

impl HostBridge for NoopHost {}
