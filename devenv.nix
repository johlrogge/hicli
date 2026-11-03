{ pkgs, ... }:
{
  # hicli — CLI-enable your code. A crates.io-publishable framework for building
  # modular CLIs: one subcommand is a documented function (`#[command]`), a group
  # of subcommands is a documented module whose children are discovered
  # (`#[group]`, `group_mod!`), and an argument is a type (`#[derive(Arg)]`) —
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

  # Stable toolchain only — hicli targets crates.io and stable-Rust
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
