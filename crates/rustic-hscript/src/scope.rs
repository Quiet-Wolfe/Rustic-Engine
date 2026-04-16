//! Lexical scope stack. Cheap to clone (each frame is an `Rc<RefCell<...>>`)
//! so closures can capture the chain they were defined in.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::value::Value;

type Frame = Rc<RefCell<HashMap<String, Value>>>;

#[derive(Clone, Default)]
pub struct Scope {
    frames: Vec<Frame>,
}

impl Scope {
    pub fn new() -> Self {
        Self {
            frames: vec![Rc::new(RefCell::new(HashMap::new()))],
        }
    }

    pub fn push(&mut self) {
        self.frames.push(Rc::new(RefCell::new(HashMap::new())));
    }

    pub fn pop(&mut self) {
        if self.frames.len() > 1 {
            self.frames.pop();
        }
    }

    /// Bind a name in the current (innermost) frame. Shadows any outer binding.
    pub fn declare(&mut self, name: &str, value: Value) {
        self.frames
            .last_mut()
            .expect("scope always has a global frame")
            .borrow_mut()
            .insert(name.to_string(), value);
    }

    /// Look up a name, searching innermost → outermost.
    pub fn get(&self, name: &str) -> Option<Value> {
        for frame in self.frames.iter().rev() {
            if let Some(v) = frame.borrow().get(name) {
                return Some(v.clone());
            }
        }
        None
    }

    /// Assign to an existing binding in the nearest frame that defines it.
    /// Returns false if the name is undefined anywhere.
    pub fn assign(&mut self, name: &str, value: Value) -> bool {
        for frame in self.frames.iter().rev() {
            let mut borrow = frame.borrow_mut();
            if borrow.contains_key(name) {
                borrow.insert(name.to_string(), value);
                return true;
            }
        }
        false
    }

    /// Set-or-create at global (outermost) frame. Used for top-level HScript
    /// `var` / function definitions so Psych callbacks are always visible.
    pub fn set_global(&mut self, name: &str, value: Value) {
        self.frames
            .first()
            .expect("scope always has a global frame")
            .borrow_mut()
            .insert(name.to_string(), value);
    }

    pub fn get_global(&self, name: &str) -> Option<Value> {
        self.frames
            .first()
            .and_then(|f| f.borrow().get(name).cloned())
    }
}
