//! The body of the `event` group, declared in `examples/files/main.rs`.
//!
//! An ordinary Rust file. The group's name and help are on the declaration over
//! there; what is here is the commands, in the order `files event --help` lists
//! them. Adding one is adding a `#[command]` function to this file — there is
//! no list anywhere to add it to as well.

use hicli::bpaf::Bpaf;
use hicli::{command, group, Arg, Failure, Outcome};

// The *types* are the import: each one carries its own parser through `Arg`,
// so the bpaf constructors that used to be imported alongside them are gone.
use crate::{Ctx, EventDir, EventName};

/// create an event
///
/// The event directory is created if it does not exist.
#[command]
pub fn create(dir: EventDir, name: EventName, ctx: &Ctx) -> Outcome {
    if dir.0.is_empty() {
        return Err(Failure::runtime("the event directory must not be empty"));
    }
    println!(
        "[home={}] created event at {} named {:?}",
        ctx.home,
        dir.0,
        name.joined()
    );
    Ok(())
}

/// list events
#[command]
pub fn list(ctx: &Ctx) -> Outcome {
    println!("[home={}] no events yet", ctx.home);
    Ok(())
}

// A nested group inside a file module is an ordinary inline `#[group]`: the two
// forms compose in either order, and only the outermost one needed a file.
/// manage audiences
///
/// An audience is a set of people invited together.
#[group]
pub mod audience {
    use super::*;

    /// Options for defining an audience.
    #[derive(Debug, Clone, Bpaf, Arg)]
    pub struct DefineOpts {
        /// freeze the roster once the audience is defined
        #[bpaf(long("seal-roster"), switch)]
        pub seal_roster: bool,
    }

    /// define an audience
    ///
    /// The audience is sealed only if you say so.
    #[command]
    pub fn define(opts: DefineOpts, dir: EventDir, name: EventName, ctx: &Ctx) -> Outcome {
        println!(
            "[home={}] audience {:?} in {} (roster sealed: {})",
            ctx.home,
            name.joined(),
            dir.0,
            opts.seal_roster
        );
        Ok(())
    }
}
