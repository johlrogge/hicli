{ pkgs, ... }:
{
  # verbandi — a crates.io-publishable framework for building modular CLIs out
  # of self-contained "Verbs" (one subcommand = one type: annotated arguments +
  # a run), composed into a command tree instead of a hand-written dispatch
  # match. Built on `bpaf` (parser-as-value combinators), which verbandi
  # re-exports.
  #
  # Shared Claude Code agents + MCP tools come from metadev (imported in
  # devenv.yaml).
  #
  # The metadev SSH MCP (and other metadev tooling) keys off METADEV_PROJECT to
  # find this project's config under ~/.metadev/projects/<project>/. Set here so
  # it is present for every shell and for the MCP servers launched from it.
  # (Takes effect on the next devenv shell / session restart.)
  env.METADEV_PROJECT = "verbandi";

  # Stable toolchain only — verbandi targets crates.io and stable-Rust
  # consumers (moniker, mdma-cli, ctx). No nightly features.
  languages.rust = {
    enable = true;
    channel = "stable";
    version = "1.97.0";
    components = [ "rustc" "cargo" "clippy" "rustfmt" "rust-analyzer" ];
  };

  # `just` runs the recipes below; also exposed to agents via the metadev
  # `just` MCP server.
  packages = with pkgs; [
    just
  ];
}
