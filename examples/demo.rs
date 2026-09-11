//! A small end-to-end `verbandi` CLI: living documentation and the demo.
//!
//! Try:
//! ```text
//! cargo run --example demo -- --help
//! cargo run --example demo -- status
//! cargo run --example demo -- event --help
//! cargo run --example demo -- event create ./dir "My Event"
//! cargo run --example demo -- event list
//! cargo run --example demo -- event creat ./dir x   # typo -> suggestion, exit 2
//! cargo run --example demo -- event create ./dir     # missing NAME -> usage, exit 2
//! ```

use std::process::ExitCode;
use verbandi::bpaf::{Bpaf, Parser};
use verbandi::{App, BoxedVerb, Exit, Group, Verb};

/// Global options, shared across every verb.
#[derive(Debug, Clone, Bpaf)]
struct Globals {
    /// Base directory for the demo's data.
    #[bpaf(long, argument("PATH"), fallback(".".into()))]
    home: String,
}

/// The shared context handed to every verb.
struct Ctx {
    home: String,
}

/// `event create <DIR> <NAME...>`
#[derive(Debug, Clone, Bpaf)]
struct Create {
    #[bpaf(positional("DIR"))]
    dir: String,
    #[bpaf(positional("NAME"), some("a name is required"))]
    name: Vec<String>,
}

impl Verb<Ctx> for Create {
    fn name(&self) -> &'static str {
        "create"
    }
    fn run(self: Box<Self>, ctx: &Ctx) -> Result<(), Exit> {
        println!(
            "[home={}] created event at {} named {:?}",
            ctx.home, self.dir, self.name
        );
        Ok(())
    }
}

fn create_cmd() -> impl Parser<BoxedVerb<Ctx>> {
    create().map(|c| Box::new(c) as BoxedVerb<Ctx>)
}

/// `event list`
#[derive(Debug, Clone, Bpaf)]
struct List {}

impl Verb<Ctx> for List {
    fn name(&self) -> &'static str {
        "list"
    }
    fn run(self: Box<Self>, ctx: &Ctx) -> Result<(), Exit> {
        println!("[home={}] no events yet", ctx.home);
        Ok(())
    }
}

fn list_cmd() -> impl Parser<BoxedVerb<Ctx>> {
    list().map(|c| Box::new(c) as BoxedVerb<Ctx>)
}

/// `status`
#[derive(Debug, Clone, Bpaf)]
struct Status {}

impl Verb<Ctx> for Status {
    fn name(&self) -> &'static str {
        "status"
    }
    fn run(self: Box<Self>, ctx: &Ctx) -> Result<(), Exit> {
        println!("[home={}] ok", ctx.home);
        Ok(())
    }
}

fn status_cmd() -> impl Parser<BoxedVerb<Ctx>> {
    status().map(|c| Box::new(c) as BoxedVerb<Ctx>)
}

/// Build the demo application.
fn app() -> App<Globals, Ctx> {
    let event = Group::new()
        .leaf("create", "create an event", create_cmd())
        .leaf("list", "list events", list_cmd());
    App::new(globals())
        .group("event", "manage events", event)
        .leaf("status", "show demo status", status_cmd())
}

fn main() -> ExitCode {
    let code = app().run(|g: Globals| Ok(Ctx { home: g.home }));
    ExitCode::from(code as u8)
}
