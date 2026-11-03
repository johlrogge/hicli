{ metadevLib, pkgs, ... }:
{
  # hicli — CLI-enable your code. A crates.io-publishable framework for building
  # modular CLIs: one subcommand is a documented function (`#[command]`), a group
  # of subcommands is a documented module whose children are discovered
  # (`#[group]`, on an inline or file module), and an argument is a type (`#[derive(Arg)]`) —
  # composed with `commands!`/`app!` into a command tree instead of a
  # hand-written dispatch match. Built on `bpaf` (parser-as-value combinators),
  # which hicli re-exports.
  #
  # Shared Claude Code agents + MCP tools come from metadev (imported in
  # devenv.yaml).
  #
  # The metadev SSH MCP (and other metadev tooling) keys off METADEV_PROJECT to
  # find this project's config under ~/.metadev/projects/<project>/. Set here so
  # it is present for every shell and for the MCP servers launched from it.
  # (Takes effect on the next devenv shell / session restart.)
  env.METADEV_PROJECT = "hicli";

  # hicli is a plain two-member cargo workspace, not a Polylith one: no
  # cargo-polylith, polylith agent or polylith MCP tools.
  metadev.polylith.enable = false;

  # Project-specific skill; the source lives outside the generated .claude/.
  claude.code.skills.architect = metadevLib.skillFromDir ./claude/skills/architect {
    description = "hicli project context for the architect — the function-first Command/Invocation model, crate layout, design contracts, and delegation workflow.";
  };

  # Stable toolchain only — hicli targets crates.io and stable-Rust
  # consumers (moniker, mdma-cli, ctx). No nightly features.
  languages.rust = {
    enable = true;
    channel = "stable";
    version = "1.99.0";
    components = [ "rustc" "cargo" "clippy" "rustfmt" "rust-analyzer" ];
  };

  # `just` runs the recipes below; also exposed to agents via the metadev
  # `just` MCP server.
  packages = with pkgs; [
    just
  ];
}
