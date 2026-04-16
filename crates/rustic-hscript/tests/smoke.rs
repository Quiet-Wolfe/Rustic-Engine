use rustic_hscript::{Interp, NoopHost, Value};

#[test]
fn runs_simple_arithmetic_function() {
    let src = r#"
        function add(a, b) {
            return a + b;
        }
    "#;

    let mut interp = Interp::new(NoopHost);
    interp.load("test.hx", src).expect("load");

    let out = interp
        .call("add", &[Value::Int(3), Value::Int(4)])
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

    let mut interp = Interp::new(NoopHost);
    interp.load("test.hx", src).expect("load");
    let out = interp
        .call("count", &[])
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

    let mut interp = Interp::new(NoopHost);
    interp.load("test.hx", src).expect("load");
    let out = interp
        .call("sumTo", &[Value::Int(5)])
        .expect("call")
        .expect("sumTo defined");
    // 0 + 1 + 2 + 3 + 4 = 10
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

    let mut interp = Interp::new(NoopHost);
    interp.load("test.hx", src).expect("load");

    let a = interp.call("bump", &[]).unwrap().unwrap();
    let b = interp.call("bump", &[]).unwrap().unwrap();

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

    let mut interp = Interp::new(NoopHost);
    interp.load("test.hx", src).expect("load");
    let out = interp
        .call("greet", &[Value::from_str("psych")])
        .unwrap()
        .unwrap();
    assert_eq!(out.as_str(), Some("hello, psych!"));
}
