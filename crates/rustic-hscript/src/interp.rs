//! Tree-walking evaluator over Rayzor's Haxe AST.
//!
//! Coverage: literals, identifiers, field/index access, call, new, unary,
//! binary, assign, ternary, array/object literals, block, var, if, while, for
//! (range + array/map iterator), return/break/continue, throw, try/catch,
//! function/closure, string interpolation.
//!
//! Deliberately unsupported: macros, reify, compiler-specific, pattern
//! matching (`switch` with destructuring), comprehensions, regex literals,
//! cast, type-check. Psych mods don't lean on these; if they do we'll surface
//! an `HScriptError::Unsupported`.
//!
//! The host is passed per-call (`&mut dyn HostBridge`) rather than owned by
//! `Interp`. That lets embedders (e.g. rustic-scripting) re-borrow their
//! own state freely across invocations without ending up in Rc<RefCell<>>
//! territory.

use std::rc::Rc;

use rayzor_parser::haxe_ast::{
    AssignOp, BinaryOp, BlockElement, Expr, ExprKind, Function, StringPart, UnaryOp,
};

use crate::host::HostBridge;
use crate::scope::Scope;
use crate::value::{Closure, Value};
use crate::{HResult, HScriptError};

/// Control-flow signal that bubbles up the evaluator. Not a user-visible
/// error; we use a plain Result<_, Flow> internally so `?` bails out of
/// nested calls cleanly.
enum Flow {
    Return(Value),
    Break,
    Continue,
    Throw(Value),
    Err(HScriptError),
}

impl From<HScriptError> for Flow {
    fn from(e: HScriptError) -> Self {
        Flow::Err(e)
    }
}

type EvalResult = Result<Value, Flow>;

/// Runtime interpreter. One instance per HScript source. Holds the global
/// scope plus top-level functions installed by `load`.
pub struct Interp {
    pub scope: Scope,
}

impl Default for Interp {
    fn default() -> Self {
        Self::new()
    }
}

impl Interp {
    pub fn new() -> Self {
        Self {
            scope: Scope::new(),
        }
    }

    /// Evaluate top-level declarations: installs functions + var initializers
    /// as globals so callbacks can be looked up later.
    pub fn load(
        &mut self,
        source_name: &str,
        source: &str,
        host: &mut dyn HostBridge,
    ) -> HResult<()> {
        let file = crate::parse(source_name, source)?;

        for (name, init) in crate::collect_top_level_vars(&file) {
            let value = match init {
                Some(expr) => self.eval_expr(&expr, host).map_err(flow_to_err)?,
                None => Value::Null,
            };
            self.scope.set_global(&name, value);
        }

        for func in crate::collect_top_level_functions(&file) {
            let name = func.name.clone();
            let closure = Closure {
                func,
                captured: self.scope.clone(),
            };
            self.scope
                .set_global(&name, Value::Closure(Rc::new(closure)));
        }

        Ok(())
    }

    /// Call a top-level function by name. Returns `Ok(None)` if the function
    /// isn't defined, `Ok(Some(_))` with the return value otherwise.
    pub fn call(
        &mut self,
        name: &str,
        args: &[Value],
        host: &mut dyn HostBridge,
    ) -> HResult<Option<Value>> {
        let callee = match self.scope.get_global(name) {
            Some(v) => v,
            None => return Ok(None),
        };
        self.call_value(&callee, args, host).map(Some)
    }

    /// Returns true if a top-level function with the given name is defined.
    pub fn has_function(&self, name: &str) -> bool {
        matches!(self.scope.get_global(name), Some(Value::Closure(_)))
    }

    /// Set/replace a script-visible global. Useful for the embedder to inject
    /// per-frame context (e.g. `curBeat`) before calling a callback.
    pub fn set_global(&mut self, name: &str, value: Value) {
        self.scope.set_global(name, value);
    }

    fn call_value(
        &mut self,
        callee: &Value,
        args: &[Value],
        host: &mut dyn HostBridge,
    ) -> HResult<Value> {
        match callee {
            Value::Closure(c) => self.call_closure(c, args, host).map_err(flow_to_err),
            Value::HostFn(f) => f(args).map_err(HScriptError::Runtime),
            _ => Err(HScriptError::Runtime(format!(
                "value is not callable: {callee:?}"
            ))),
        }
    }

    fn call_closure(
        &mut self,
        c: &Rc<Closure>,
        args: &[Value],
        host: &mut dyn HostBridge,
    ) -> EvalResult {
        let saved = std::mem::replace(&mut self.scope, c.captured.clone());
        self.scope.push();
        for (i, param) in c.func.params.iter().enumerate() {
            let value = args.get(i).cloned().unwrap_or(Value::Null);
            self.scope.declare(&param.name, value);
        }
        let result = match &c.func.body {
            Some(body) => match self.eval_expr(body, host) {
                Ok(v) => Ok(v),
                Err(Flow::Return(v)) => Ok(v),
                Err(other) => Err(other),
            },
            None => Ok(Value::Null),
        };
        self.scope.pop();
        self.scope = saved;
        result
    }

    fn eval_expr(&mut self, expr: &Expr, host: &mut dyn HostBridge) -> EvalResult {
        match &expr.kind {
            ExprKind::Int(i) => Ok(Value::Int(*i)),
            ExprKind::Float(f) => Ok(Value::Float(*f)),
            ExprKind::String(s) => Ok(Value::from_str(s.clone())),
            ExprKind::Bool(b) => Ok(Value::Bool(*b)),
            ExprKind::Null => Ok(Value::Null),
            ExprKind::This => Ok(self.scope.get("this").unwrap_or(Value::Null)),
            ExprKind::Super => Ok(self.scope.get("super").unwrap_or(Value::Null)),

            ExprKind::Ident(name) => self.lookup_ident(name, host),

            ExprKind::Field {
                expr,
                field,
                is_optional,
            } => {
                let target = self.eval_expr(expr, host)?;
                if *is_optional && matches!(target, Value::Null) {
                    return Ok(Value::Null);
                }
                self.field_get(&target, field, host)
            }

            ExprKind::Index { expr, index } => {
                let target = self.eval_expr(expr, host)?;
                let idx = self.eval_expr(index, host)?;
                self.index_get(&target, &idx)
            }

            ExprKind::Call { expr, args } => self.eval_call(expr, args, host),

            ExprKind::New {
                type_path, args, ..
            } => {
                let evaluated: Vec<Value> = args
                    .iter()
                    .map(|a| self.eval_expr(a, host))
                    .collect::<Result<_, _>>()?;
                let name = type_path.name.as_str();
                host.construct(name, &evaluated)
                    .map_err(|e| Flow::Err(HScriptError::Runtime(e)))
            }

            ExprKind::Unary { op, expr } => self.eval_unary(*op, expr, host),
            ExprKind::Binary { left, op, right } => self.eval_binary(left, *op, right, host),
            ExprKind::Assign { left, op, right } => self.eval_assign(left, *op, right, host),

            ExprKind::Ternary {
                cond,
                then_expr,
                else_expr,
            } => {
                if self.eval_expr(cond, host)?.is_truthy() {
                    self.eval_expr(then_expr, host)
                } else {
                    self.eval_expr(else_expr, host)
                }
            }

            ExprKind::Array(elements) => {
                let values: Vec<Value> = elements
                    .iter()
                    .map(|e| self.eval_expr(e, host))
                    .collect::<Result<_, _>>()?;
                Ok(Value::new_array(values))
            }

            ExprKind::Object(fields) => {
                let obj = Value::new_object();
                if let Value::Object(map) = &obj {
                    for f in fields {
                        let v = self.eval_expr(&f.expr, host)?;
                        map.borrow_mut().insert(f.name.clone(), v);
                    }
                }
                Ok(obj)
            }

            ExprKind::StringInterpolation(parts) => {
                let mut out = String::new();
                for part in parts {
                    match part {
                        StringPart::Literal(s) => out.push_str(s),
                        StringPart::Interpolation(e) => {
                            let v = self.eval_expr(e, host)?;
                            out.push_str(&format!("{v}"));
                        }
                    }
                }
                Ok(Value::from_str(out))
            }

            ExprKind::Block(elements) => self.eval_block(elements, host),

            ExprKind::Var { name, expr, .. } | ExprKind::Final { name, expr, .. } => {
                let value = match expr {
                    Some(e) => self.eval_expr(e, host)?,
                    None => Value::Null,
                };
                self.scope.declare(name, value);
                Ok(Value::Null)
            }

            ExprKind::Function(func) => {
                let closure = Closure {
                    func: func.clone(),
                    captured: self.scope.clone(),
                };
                let value = Value::Closure(Rc::new(closure));
                if !func.name.is_empty() {
                    self.scope.declare(&func.name, value.clone());
                }
                Ok(value)
            }

            ExprKind::Arrow { params, expr } => {
                let synthetic = Function {
                    name: String::new(),
                    type_params: Vec::new(),
                    params: params
                        .iter()
                        .map(|p| rayzor_parser::haxe_ast::FunctionParam {
                            meta: Vec::new(),
                            name: p.name.clone(),
                            type_hint: p.type_hint.clone(),
                            optional: false,
                            rest: false,
                            default_value: None,
                            span: expr.span,
                        })
                        .collect(),
                    return_type: None,
                    body: Some(expr.clone()),
                    span: expr.span,
                };
                let closure = Closure {
                    func: synthetic,
                    captured: self.scope.clone(),
                };
                Ok(Value::Closure(Rc::new(closure)))
            }

            ExprKind::Return(opt) => {
                let v = match opt {
                    Some(e) => self.eval_expr(e, host)?,
                    None => Value::Null,
                };
                Err(Flow::Return(v))
            }
            ExprKind::Break => Err(Flow::Break),
            ExprKind::Continue => Err(Flow::Continue),
            ExprKind::Throw(e) => {
                let v = self.eval_expr(e, host)?;
                Err(Flow::Throw(v))
            }

            ExprKind::If {
                cond,
                then_branch,
                else_branch,
            } => {
                if self.eval_expr(cond, host)?.is_truthy() {
                    self.eval_expr(then_branch, host)
                } else if let Some(else_b) = else_branch {
                    self.eval_expr(else_b, host)
                } else {
                    Ok(Value::Null)
                }
            }

            ExprKind::While { cond, body } => {
                while self.eval_expr(cond, host)?.is_truthy() {
                    match self.eval_expr(body, host) {
                        Ok(_) => {}
                        Err(Flow::Break) => break,
                        Err(Flow::Continue) => continue,
                        Err(other) => return Err(other),
                    }
                }
                Ok(Value::Null)
            }

            ExprKind::DoWhile { body, cond } => {
                loop {
                    match self.eval_expr(body, host) {
                        Ok(_) => {}
                        Err(Flow::Break) => break,
                        Err(Flow::Continue) => {}
                        Err(other) => return Err(other),
                    }
                    if !self.eval_expr(cond, host)?.is_truthy() {
                        break;
                    }
                }
                Ok(Value::Null)
            }

            ExprKind::For {
                var,
                key_var,
                iter,
                body,
            } => self.eval_for(var, key_var.as_deref(), iter, body, host),

            ExprKind::Try { expr, catches, .. } => match self.eval_expr(expr, host) {
                Err(Flow::Throw(thrown)) => {
                    if let Some(catch) = catches.first() {
                        self.scope.push();
                        self.scope.declare(&catch.var, thrown);
                        let result = self.eval_expr(&catch.body, host);
                        self.scope.pop();
                        result
                    } else {
                        Err(Flow::Throw(thrown))
                    }
                }
                other => other,
            },

            ExprKind::Paren(inner) => self.eval_expr(inner, host),
            ExprKind::Cast { expr, .. } => self.eval_expr(expr, host),
            ExprKind::TypeCheck { expr, .. } => self.eval_expr(expr, host),
            ExprKind::Untyped(inner) => self.eval_expr(inner, host),
            ExprKind::Meta { expr, .. } => self.eval_expr(expr, host),
            ExprKind::Inline(inner) => self.eval_expr(inner, host),

            _ => Err(Flow::Err(HScriptError::Unsupported(
                "expression kind not supported in HScript interpreter",
            ))),
        }
    }

    fn eval_block(&mut self, elements: &[BlockElement], host: &mut dyn HostBridge) -> EvalResult {
        self.scope.push();
        let mut last = Value::Null;
        let mut result: EvalResult = Ok(Value::Null);
        for el in elements {
            match el {
                BlockElement::Expr(e) => match self.eval_expr(e, host) {
                    Ok(v) => last = v,
                    Err(f) => {
                        result = Err(f);
                        break;
                    }
                },
                BlockElement::Import(_) | BlockElement::Using(_) | BlockElement::Conditional(_) => {
                    // Compile-time directives — ignored at runtime.
                }
            }
        }
        self.scope.pop();
        match result {
            Ok(_) => Ok(last),
            Err(f) => Err(f),
        }
    }

    fn eval_for(
        &mut self,
        var: &str,
        key_var: Option<&str>,
        iter: &Expr,
        body: &Expr,
        host: &mut dyn HostBridge,
    ) -> EvalResult {
        // Range literal `a...b` shows up as a Binary with op Range.
        if let ExprKind::Binary {
            left,
            op: BinaryOp::Range,
            right,
        } = &iter.kind
        {
            let start = self.eval_expr(left, host)?.as_i64().ok_or_else(|| {
                Flow::Err(HScriptError::Runtime("range start not integer".into()))
            })?;
            let end = self
                .eval_expr(right, host)?
                .as_i64()
                .ok_or_else(|| Flow::Err(HScriptError::Runtime("range end not integer".into())))?;
            for i in start..end {
                self.scope.push();
                self.scope.declare(var, Value::Int(i));
                let step = self.eval_expr(body, host);
                self.scope.pop();
                match step {
                    Ok(_) => {}
                    Err(Flow::Break) => return Ok(Value::Null),
                    Err(Flow::Continue) => continue,
                    Err(other) => return Err(other),
                }
            }
            return Ok(Value::Null);
        }

        let target = self.eval_expr(iter, host)?;
        match target {
            Value::Array(arr) => {
                let snapshot: Vec<Value> = arr.borrow().clone();
                for (idx, item) in snapshot.into_iter().enumerate() {
                    self.scope.push();
                    if let Some(k) = key_var {
                        self.scope.declare(k, Value::Int(idx as i64));
                    }
                    self.scope.declare(var, item);
                    let step = self.eval_expr(body, host);
                    self.scope.pop();
                    match step {
                        Ok(_) => {}
                        Err(Flow::Break) => return Ok(Value::Null),
                        Err(Flow::Continue) => continue,
                        Err(other) => return Err(other),
                    }
                }
                Ok(Value::Null)
            }
            Value::Object(map) => {
                let snapshot: Vec<(String, Value)> = map
                    .borrow()
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                for (k, v) in snapshot {
                    self.scope.push();
                    if let Some(kn) = key_var {
                        self.scope.declare(kn, Value::from_str(k));
                        self.scope.declare(var, v);
                    } else {
                        self.scope.declare(var, Value::from_str(k));
                        let _ = v;
                    }
                    let step = self.eval_expr(body, host);
                    self.scope.pop();
                    match step {
                        Ok(_) => {}
                        Err(Flow::Break) => return Ok(Value::Null),
                        Err(Flow::Continue) => continue,
                        Err(other) => return Err(other),
                    }
                }
                Ok(Value::Null)
            }
            other => Err(Flow::Err(HScriptError::Runtime(format!(
                "cannot iterate over {other:?}"
            )))),
        }
    }

    fn lookup_ident(&mut self, name: &str, host: &mut dyn HostBridge) -> EvalResult {
        if let Some(v) = self.scope.get(name) {
            return Ok(v);
        }
        host.global_get(name)
            .map_err(|e| Flow::Err(HScriptError::Runtime(e)))
    }

    fn field_get(&mut self, target: &Value, field: &str, host: &mut dyn HostBridge) -> EvalResult {
        match target {
            Value::Object(map) => Ok(map.borrow().get(field).cloned().unwrap_or(Value::Null)),
            Value::Array(arr) => match field {
                "length" => Ok(Value::Int(arr.borrow().len() as i64)),
                _ => host
                    .field_get(target, field)
                    .map_err(|e| Flow::Err(HScriptError::Runtime(e))),
            },
            Value::String(s) => match field {
                "length" => Ok(Value::Int(s.chars().count() as i64)),
                _ => host
                    .field_get(target, field)
                    .map_err(|e| Flow::Err(HScriptError::Runtime(e))),
            },
            _ => host
                .field_get(target, field)
                .map_err(|e| Flow::Err(HScriptError::Runtime(e))),
        }
    }

    fn field_set(
        &mut self,
        target: &Value,
        field: &str,
        value: Value,
        host: &mut dyn HostBridge,
    ) -> EvalResult {
        match target {
            Value::Object(map) => {
                map.borrow_mut().insert(field.to_string(), value.clone());
                Ok(value)
            }
            _ => {
                host.field_set(target, field, &value)
                    .map_err(|e| Flow::Err(HScriptError::Runtime(e)))?;
                Ok(value)
            }
        }
    }

    fn index_get(&mut self, target: &Value, index: &Value) -> EvalResult {
        match target {
            Value::Array(arr) => {
                let i = index.as_i64().ok_or_else(|| {
                    Flow::Err(HScriptError::Runtime("array index not integer".into()))
                })?;
                if i < 0 {
                    return Ok(Value::Null);
                }
                Ok(arr.borrow().get(i as usize).cloned().unwrap_or(Value::Null))
            }
            Value::Object(map) => {
                let key = match index {
                    Value::String(s) => s.as_str().to_string(),
                    other => format!("{other}"),
                };
                Ok(map.borrow().get(&key).cloned().unwrap_or(Value::Null))
            }
            _ => Err(Flow::Err(HScriptError::Runtime(format!(
                "cannot index into {target:?}"
            )))),
        }
    }

    fn index_set(&mut self, target: &Value, index: &Value, value: Value) -> EvalResult {
        match target {
            Value::Array(arr) => {
                let i = index.as_i64().ok_or_else(|| {
                    Flow::Err(HScriptError::Runtime("array index not integer".into()))
                })?;
                if i < 0 {
                    return Err(Flow::Err(HScriptError::Runtime(
                        "negative array index".into(),
                    )));
                }
                let mut vec = arr.borrow_mut();
                if (i as usize) >= vec.len() {
                    vec.resize((i as usize) + 1, Value::Null);
                }
                vec[i as usize] = value.clone();
                Ok(value)
            }
            Value::Object(map) => {
                let key = match index {
                    Value::String(s) => s.as_str().to_string(),
                    other => format!("{other}"),
                };
                map.borrow_mut().insert(key, value.clone());
                Ok(value)
            }
            _ => Err(Flow::Err(HScriptError::Runtime(format!(
                "cannot assign index on {target:?}"
            )))),
        }
    }

    fn eval_call(&mut self, callee: &Expr, args: &[Expr], host: &mut dyn HostBridge) -> EvalResult {
        let arg_values: Vec<Value> = args
            .iter()
            .map(|a| self.eval_expr(a, host))
            .collect::<Result<_, _>>()?;

        // Method call on a field access (obj.method(args)) — dispatch through the
        // host so it can implement e.g. sprite.playAnim().
        if let ExprKind::Field { expr, field, .. } = &callee.kind {
            let target = self.eval_expr(expr, host)?;
            if let Value::Object(map) = &target {
                if let Some(inner) = map.borrow().get(field).cloned() {
                    return self
                        .call_value(&inner, &arg_values, host)
                        .map_err(Flow::Err);
                }
            }
            return host
                .method_call(&target, field, &arg_values)
                .map_err(|e| Flow::Err(HScriptError::Runtime(e)));
        }

        let callee_value = self.eval_expr(callee, host)?;
        self.call_value(&callee_value, &arg_values, host)
            .map_err(Flow::Err)
    }

    fn eval_unary(&mut self, op: UnaryOp, expr: &Expr, host: &mut dyn HostBridge) -> EvalResult {
        match op {
            UnaryOp::Not => {
                let v = self.eval_expr(expr, host)?;
                Ok(Value::Bool(!v.is_truthy()))
            }
            UnaryOp::Neg => {
                let v = self.eval_expr(expr, host)?;
                match v {
                    Value::Int(i) => Ok(Value::Int(-i)),
                    Value::Float(f) => Ok(Value::Float(-f)),
                    other => Err(Flow::Err(HScriptError::Runtime(format!(
                        "cannot negate {other:?}"
                    )))),
                }
            }
            UnaryOp::BitNot => {
                let v = self.eval_expr(expr, host)?;
                Ok(Value::Int(!v.as_i64().unwrap_or(0)))
            }
            UnaryOp::PreIncr | UnaryOp::PreDecr | UnaryOp::PostIncr | UnaryOp::PostDecr => {
                let before = self.eval_expr(expr, host)?;
                let delta = match op {
                    UnaryOp::PreIncr | UnaryOp::PostIncr => 1.0,
                    UnaryOp::PreDecr | UnaryOp::PostDecr => -1.0,
                    _ => unreachable!(),
                };
                let after = match &before {
                    Value::Int(i) => Value::Int(*i + delta as i64),
                    Value::Float(f) => Value::Float(*f + delta),
                    other => {
                        return Err(Flow::Err(HScriptError::Runtime(format!(
                            "cannot increment {other:?}"
                        ))));
                    }
                };
                self.store(expr, after.clone(), host)?;
                match op {
                    UnaryOp::PreIncr | UnaryOp::PreDecr => Ok(after),
                    UnaryOp::PostIncr | UnaryOp::PostDecr => Ok(before),
                    _ => unreachable!(),
                }
            }
        }
    }

    fn eval_binary(
        &mut self,
        left: &Expr,
        op: BinaryOp,
        right: &Expr,
        host: &mut dyn HostBridge,
    ) -> EvalResult {
        // Short-circuit for logical ops.
        match op {
            BinaryOp::And => {
                let l = self.eval_expr(left, host)?;
                if !l.is_truthy() {
                    return Ok(l);
                }
                return self.eval_expr(right, host);
            }
            BinaryOp::Or => {
                let l = self.eval_expr(left, host)?;
                if l.is_truthy() {
                    return Ok(l);
                }
                return self.eval_expr(right, host);
            }
            BinaryOp::NullCoal => {
                let l = self.eval_expr(left, host)?;
                if matches!(l, Value::Null) {
                    return self.eval_expr(right, host);
                }
                return Ok(l);
            }
            _ => {}
        }

        let l = self.eval_expr(left, host)?;
        let r = self.eval_expr(right, host)?;
        numeric_binop(op, l, r)
    }

    fn eval_assign(
        &mut self,
        left: &Expr,
        op: AssignOp,
        right: &Expr,
        host: &mut dyn HostBridge,
    ) -> EvalResult {
        let rhs = self.eval_expr(right, host)?;
        let new_value = if matches!(op, AssignOp::Assign) {
            rhs
        } else {
            let current = self.eval_expr(left, host)?;
            let bop = match op {
                AssignOp::AddAssign => BinaryOp::Add,
                AssignOp::SubAssign => BinaryOp::Sub,
                AssignOp::MulAssign => BinaryOp::Mul,
                AssignOp::DivAssign => BinaryOp::Div,
                AssignOp::ModAssign => BinaryOp::Mod,
                AssignOp::AndAssign => BinaryOp::BitAnd,
                AssignOp::OrAssign => BinaryOp::BitOr,
                AssignOp::XorAssign => BinaryOp::BitXor,
                AssignOp::ShlAssign => BinaryOp::Shl,
                AssignOp::ShrAssign => BinaryOp::Shr,
                AssignOp::UshrAssign => BinaryOp::Ushr,
                AssignOp::Assign => unreachable!(),
            };
            numeric_binop(bop, current, rhs)?
        };
        self.store(left, new_value.clone(), host)?;
        Ok(new_value)
    }

    fn store(&mut self, target: &Expr, value: Value, host: &mut dyn HostBridge) -> EvalResult {
        match &target.kind {
            ExprKind::Ident(name) => {
                if self.scope.assign(name, value.clone()) {
                    return Ok(value);
                }
                match host.global_set(name, &value) {
                    Ok(true) => Ok(value),
                    Ok(false) => {
                        self.scope.declare(name, value.clone());
                        Ok(value)
                    }
                    Err(e) => Err(Flow::Err(HScriptError::Runtime(e))),
                }
            }
            ExprKind::Field { expr, field, .. } => {
                let obj = self.eval_expr(expr, host)?;
                self.field_set(&obj, field, value, host)
            }
            ExprKind::Index { expr, index } => {
                let obj = self.eval_expr(expr, host)?;
                let idx = self.eval_expr(index, host)?;
                self.index_set(&obj, &idx, value)
            }
            ExprKind::Paren(inner) => self.store(inner, value, host),
            _ => Err(Flow::Err(HScriptError::Runtime(
                "invalid assignment target".into(),
            ))),
        }
    }
}

fn flow_to_err(flow: Flow) -> HScriptError {
    match flow {
        Flow::Err(e) => e,
        Flow::Return(_) => HScriptError::Runtime("return outside function".into()),
        Flow::Break => HScriptError::Runtime("break outside loop".into()),
        Flow::Continue => HScriptError::Runtime("continue outside loop".into()),
        Flow::Throw(v) => HScriptError::Runtime(format!("uncaught throw: {v:?}")),
    }
}

fn numeric_binop(op: BinaryOp, l: Value, r: Value) -> EvalResult {
    use BinaryOp::*;

    // String concat with +
    if matches!(op, Add) {
        if let (Value::String(a), b) = (&l, &r) {
            return Ok(Value::from_str(format!("{}{}", a.as_str(), b)));
        }
        if let (a, Value::String(b)) = (&l, &r) {
            return Ok(Value::from_str(format!("{}{}", a, b.as_str())));
        }
    }

    match op {
        Eq => return Ok(Value::Bool(l.loose_eq(&r))),
        NotEq => return Ok(Value::Bool(!l.loose_eq(&r))),
        _ => {}
    }

    let ln = l.as_f64();
    let rn = r.as_f64();

    let both_ints = matches!(l, Value::Int(_)) && matches!(r, Value::Int(_));
    let li = l.as_i64();
    let ri = r.as_i64();

    let as_num = |f: f64| -> Value {
        if both_ints && f.fract() == 0.0 {
            Value::Int(f as i64)
        } else {
            Value::Float(f)
        }
    };

    match op {
        Add => num2(ln, rn, |a, b| as_num(a + b)),
        Sub => num2(ln, rn, |a, b| as_num(a - b)),
        Mul => num2(ln, rn, |a, b| as_num(a * b)),
        Div => num2(ln, rn, |a, b| {
            if b == 0.0 {
                Value::Float(f64::NAN)
            } else {
                Value::Float(a / b)
            }
        }),
        Mod => num2(ln, rn, |a, b| as_num(a % b)),

        Lt => num2(ln, rn, |a, b| Value::Bool(a < b)),
        Le => num2(ln, rn, |a, b| Value::Bool(a <= b)),
        Gt => num2(ln, rn, |a, b| Value::Bool(a > b)),
        Ge => num2(ln, rn, |a, b| Value::Bool(a >= b)),

        BitAnd => int2(li, ri, |a, b| Value::Int(a & b)),
        BitOr => int2(li, ri, |a, b| Value::Int(a | b)),
        BitXor => int2(li, ri, |a, b| Value::Int(a ^ b)),
        Shl => int2(li, ri, |a, b| Value::Int(a << (b & 63))),
        Shr => int2(li, ri, |a, b| Value::Int(a >> (b & 63))),
        Ushr => int2(li, ri, |a, b| {
            Value::Int(((a as u64) >> ((b as u64) & 63)) as i64)
        }),

        Range => Err(Flow::Err(HScriptError::Runtime(
            "range operator is only valid as a for-loop iterator".into(),
        ))),
        Arrow => Err(Flow::Err(HScriptError::Runtime(
            "=> only valid inside map literals".into(),
        ))),
        Is => Err(Flow::Err(HScriptError::Unsupported(
            "'is' type check is not implemented",
        ))),

        And | Or | NullCoal | Eq | NotEq => unreachable!("handled above"),
    }
}

fn num2(a: Option<f64>, b: Option<f64>, f: impl Fn(f64, f64) -> Value) -> EvalResult {
    match (a, b) {
        (Some(a), Some(b)) => Ok(f(a, b)),
        _ => Err(Flow::Err(HScriptError::Runtime(
            "non-numeric operand".into(),
        ))),
    }
}

fn int2(a: Option<i64>, b: Option<i64>, f: impl Fn(i64, i64) -> Value) -> EvalResult {
    match (a, b) {
        (Some(a), Some(b)) => Ok(f(a, b)),
        _ => Err(Flow::Err(HScriptError::Runtime(
            "non-integer operand".into(),
        ))),
    }
}
