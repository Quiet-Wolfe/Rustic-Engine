use rustic_hscript::{Interp, NoopHost, Value};

fn make() -> (Interp, NoopHost) {
    (Interp::new(), NoopHost)
}

#[test]
fn runs_simple_arithmetic_function() {
    let src = r#"
        function add(a, b) {
            return a + b;
        }
    "#;

    let (mut interp, mut host) = make();
    interp.load("test.hx", src, &mut host).expect("load");

    let out = interp
        .call("add", &[Value::Int(3), Value::Int(4)], &mut host)
        .expect("call")
        .expect("add defined");

    assert!(matches!(out, Value::Int(7)), "expected Int(7), got {out:?}");
}

#[test]
fn while_loop_with_break() {
    let src = r#"
        function count() {
            var i = 0;
            while (true) {
                i = i + 1;
                if (i >= 5) break;
            }
            return i;
        }
    "#;

    let (mut interp, mut host) = make();
    interp.load("test.hx", src, &mut host).expect("load");
    let out = interp
        .call("count", &[], &mut host)
        .expect("call")
        .expect("count defined");
    assert!(matches!(out, Value::Int(5)), "got {out:?}");
}

#[test]
fn for_range_loop_sums() {
    let src = r#"
        function sumTo(n) {
            var total = 0;
            for (i in 0...n) {
                total += i;
            }
            return total;
        }
    "#;

    let (mut interp, mut host) = make();
    interp.load("test.hx", src, &mut host).expect("load");
    let out = interp
        .call("sumTo", &[Value::Int(5)], &mut host)
        .expect("call")
        .expect("sumTo defined");
    assert!(matches!(out, Value::Int(10)), "got {out:?}");
}

#[test]
fn top_level_var_is_captured_by_function() {
    let src = r#"
        var counter = 0;
        function bump() {
            counter = counter + 1;
            return counter;
        }
    "#;

    let (mut interp, mut host) = make();
    interp.load("test.hx", src, &mut host).expect("load");

    let a = interp.call("bump", &[], &mut host).unwrap().unwrap();
    let b = interp.call("bump", &[], &mut host).unwrap().unwrap();

    assert!(matches!(a, Value::Int(1)), "first bump: {a:?}");
    assert!(matches!(b, Value::Int(2)), "second bump: {b:?}");
}

#[test]
fn string_interpolation_works() {
    let src = r#"
        function greet(name) {
            return 'hello, $name!';
        }
    "#;

    let (mut interp, mut host) = make();
    interp.load("test.hx", src, &mut host).expect("load");
    let out = interp
        .call("greet", &[Value::from_str("psych")], &mut host)
        .unwrap()
        .unwrap();
    assert_eq!(out.as_str(), Some("hello, psych!"));
}

/// Host with a mutable counter that scripts can read/write as `hostCounter`.
/// Exercises the per-call host pattern.
#[derive(Default)]
struct CountingHost {
    counter: i64,
}

impl rustic_hscript::HostBridge for CountingHost {
    fn global_get(&mut self, name: &str) -> Result<Value, String> {
        match name {
            "hostCounter" => Ok(Value::Int(self.counter)),
            _ => Ok(Value::Null),
        }
    }

    fn global_set(&mut self, name: &str, value: &Value) -> Result<bool, String> {
        if name == "hostCounter" {
            self.counter = value.as_i64().unwrap_or(0);
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[test]
fn host_globals_round_trip() {
    let src = r#"
        function bumpHost() {
            hostCounter = hostCounter + 1;
            return hostCounter;
        }
    "#;

    let mut interp = Interp::new();
    let mut host = CountingHost::default();
    interp.load("test.hx", src, &mut host).expect("load");

    let a = interp.call("bumpHost", &[], &mut host).unwrap().unwrap();
    let b = interp.call("bumpHost", &[], &mut host).unwrap().unwrap();

    assert!(matches!(a, Value::Int(1)), "first: {a:?}");
    assert!(matches!(b, Value::Int(2)), "second: {b:?}");
    assert_eq!(host.counter, 2, "host saw the write");
}

#[test]
fn builtin_array_and_string_methods_match_hscript_expectations() {
    let src = r#"
        function exercise() {
            var values = [];
            values.push("ONE");
            values.push("two");
            values.insert(1, "middle");
            values.remove("two");
            return values.join("|") + ":" + "  AbC  ".trim().toLowerCase();
        }
    "#;

    let (mut interp, mut host) = make();
    interp.load("test.hx", src, &mut host).expect("load");
    let out = interp.call("exercise", &[], &mut host).unwrap().unwrap();
    assert_eq!(out.as_str(), Some("ONE|middle:abc"));
}

#[test]
fn switch_supports_const_default_and_variable_patterns() {
    let src = r#"
        function label(v) {
            switch (v) {
                case 1: return "one";
                case "bf": return "boyfriend";
                case captured: return "got " + captured;
                default: return "none";
            }
        }
    "#;

    let (mut interp, mut host) = make();
    interp.load("test.hx", src, &mut host).expect("load");
    let one = interp
        .call("label", &[Value::Int(1)], &mut host)
        .unwrap()
        .unwrap();
    let bf = interp
        .call("label", &[Value::from_str("bf")], &mut host)
        .unwrap()
        .unwrap();
    let any = interp
        .call("label", &[Value::Int(7)], &mut host)
        .unwrap()
        .unwrap();
    assert_eq!(one.as_str(), Some("one"));
    assert_eq!(bf.as_str(), Some("boyfriend"));
    assert_eq!(any.as_str(), Some("got 7"));
}

#[test]
fn map_literals_behave_like_dynamic_lookup_objects() {
    let src = r#"
        function lookup(name) {
            var values = ["bf" => 10, "dad" => 20];
            values["gf"] = 30;
            var total = 0;
            for (key => value in values) {
                total += value;
            }
            return values[name] + total;
        }
    "#;

    let (mut interp, mut host) = make();
    interp.load("test.hx", src, &mut host).expect("load");
    let out = interp
        .call("lookup", &[Value::from_str("dad")], &mut host)
        .unwrap()
        .unwrap();
    assert!(matches!(out, Value::Int(80)), "got {out:?}");
}
