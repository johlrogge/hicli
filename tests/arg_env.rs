//! `env = "VAR"` with the variable actually set (ADR-019 §5): a good value
//! parses, a bad one is named like the option, and under `Option<T>` a value
//! from the environment counts as given.
//!
//! The variable is set by the one test in this binary, so nothing else in the
//! process reads the environment while it changes.

use std::cell::RefCell;
use std::net::SocketAddr;

use hicli::bpaf::pure;
use hicli::{app, cli, command, exit_code_for, App, Arg};

/// Records what the command was handed.
#[derive(Default)]
pub struct Ctx {
    seen: RefCell<Option<Option<Addr>>>,
}

/// The node's address
#[derive(Debug, Clone, Copy, PartialEq, Eq, Arg)]
#[arg(long = "addr", metavar = "ADDR", env = "HICLI_ARG_ENV_TEST_ADDR")]
pub struct Addr(pub SocketAddr);

/// Connect to the node
#[command]
pub fn connect(addr: Option<Addr>, ctx: &Ctx) {
    *ctx.seen.borrow_mut() = Some(addr);
}

fn app() -> App<(), Ctx> {
    app! { globals: pure(()), commands: [connect] }
}

/// What `connect` was handed.
fn handed(args: Vec<String>) -> Option<Addr> {
    let (_, inv) = app().parse(args).expect("parses");
    let ctx = Ctx::default();
    inv.run(&ctx).expect("runs");
    ctx.seen.take().expect("connect ran")
}

#[test]
fn an_environment_value_is_parsed_named_and_counts_as_given() {
    const VAR: &str = "HICLI_ARG_ENV_TEST_ADDR";
    let addr = |s: &str| Addr(s.parse().expect("an address"));

    // Edition 2021: `set_var` is a safe function, and this crate forbids
    // `unsafe`. Sound here because this is the only test in its binary, so no
    // other thread reads or writes the environment meanwhile.
    std::env::remove_var(VAR);
    assert_eq!(handed(cli! { connect }), None);

    std::env::set_var(VAR, "127.0.0.1:7700");
    assert_eq!(handed(cli! { connect }), Some(addr("127.0.0.1:7700")));
    // Set, `--help` prints the value — which is why no secret goes in a
    // parser field (README).
    let help = app()
        .parse(cli! { connect --help })
        .err()
        .expect("help")
        .unwrap_stdout();
    assert!(
        help.contains("[env:HICLI_ARG_ENV_TEST_ADDR = \"127.0.0.1:7700\"]"),
        "{help}"
    );
    // The command line still wins.
    assert_eq!(
        handed(cli! { connect --addr 127.0.0.1:1 }),
        Some(addr("127.0.0.1:1"))
    );

    std::env::set_var(VAR, "x");
    let e = app().parse(cli! { connect }).err().expect("a usage error");
    assert_eq!(exit_code_for(&e), 2);
    // bpaf prints no word for a value that came from the environment, only
    // `couldn't parse:`; hicli names the option and the variable it came from,
    // and never echoes the value — the environment is where secrets live.
    assert_eq!(
        e.unwrap_stderr(),
        "couldn't parse: `--addr` from `$HICLI_ARG_ENV_TEST_ADDR`: invalid socket address syntax"
    );
    // The same word typed on the command line is named as the option only.
    let e = app()
        .parse(cli! { connect --addr y })
        .err()
        .expect("a usage error");
    assert_eq!(
        e.unwrap_stderr(),
        "couldn't parse `y`: `--addr`: invalid socket address syntax"
    );

    std::env::remove_var(VAR);
}
