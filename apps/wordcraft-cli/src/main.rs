//! `wordcraft-cli`: WordCraft from the command line.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod automation_args;

use std::fmt;
use std::io::Write;
use std::process::ExitCode;

use serde_json::{Value, json};
use wordcraft_engine::Session;

const USAGE: &str = "\
wordcraft-cli — WordCraft from the command line

USAGE:
  wordcraft-cli convert <in> <out> [--markup]
                                              convert between formats (docx, doc [read], pdf, odt, rtf, html, md, tex, txt, json, png);
                                              --markup (Print Markup): comments beside each page and
                                              tracked changes in pdf/png, a comment list in html/md/txt
  wordcraft-cli info <file>                   pages, words, paragraphs, properties (JSON)
  wordcraft-cli text <file>                   plain text
  wordcraft-cli inspect <file>                document structure (JSON)
  wordcraft-cli render <file> <out.png> [--page N] [--scale S]
  wordcraft-cli run [--file F | --template T] --cmd 'id={json}' [--cmd …] [--save OUT] [--print]
                    [--author NAME]           run commands headlessly, then save (--author names
                                              who tracked changes and comments are by)
  wordcraft-cli commands [--json]             list every command
  wordcraft-cli parity [--markdown]           feature-catalog parity
  wordcraft-cli mcp [--connect HOST:PORT | --author NAME] [--automation-read-root DIR] [--automation-write-root DIR]
                                              MCP server on stdio (headless, or bridged to the app).
                                              --automation-read-root / --automation-write-root confine
                                              the files the agent's commands read (open, insert text
                                              or picture, mail-merge data, templates…) and write (save,
                                              export, merge output…) to DIR, links followed; a root left
                                              out grants none of its access. They apply to the headless
                                              server and are an error with --connect.
  wordcraft-cli zotero <command> <file> [--cmd 'id={json}' …] [--save OUT] [--trace] [--port P]
                                              run a Zotero command on a document (Zotero must be
                                              running): addEditCitation, addEditBibliography,
                                              addNote, refresh, removeCodes, setDocPrefs
                                              (--cmd runs first, e.g. --cmd caret.docEnd)
  wordcraft-cli --version
";

fn open(path: &str) -> Result<Session, String> {
    let doc = wordcraft_engine::io::open_path(std::path::Path::new(path))?;
    let mut s = Session::new(doc);
    s.path = Some(path.into());
    Ok(s)
}

/// `--author NAME`: the author recorded on tracked changes and comments (`file.setAuthor`).
fn set_author(s: &mut Session, name: &str) -> Result<(), String> {
    s.run("file.setAuthor", &json!({"name": name})).map(|_| ()).map_err(|e| format!("--author: {e}"))
}

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

#[derive(Clone, Copy)]
struct OptionSpec {
    name: &'static str,
    takes_value: bool,
}

fn validate_options(command: &str, args: &[String], specs: &[OptionSpec]) -> Result<(), String> {
    let mut i = 0;
    while let Some(arg) = args.get(i) {
        if let Some(name) = arg.strip_prefix("--") {
            let Some(spec) = specs.iter().find(|spec| spec.name == arg) else {
                return Err(format!("{command}: unknown option --{name}"));
            };
            if spec.takes_value && args.get(i + 1).is_some_and(|value| !value.starts_with("--")) {
                i += 2;
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    Ok(())
}

/// Take `--automation-read-root` / `--automation-write-root` (each `--flag DIR` or `--flag=DIR`,
/// once) out of `args`, and open the roots, which must be existing folders. Returns the arguments
/// that are left. A malformed root flag is an error, never a root silently left out.
fn automation_roots(
    command: &str,
    args: &[String],
) -> Result<(Option<std::sync::Arc<wordcraft_engine::file_access::AutomationRoots>>, Vec<String>), String> {
    let mut found = automation_args::AutomationArgs::new(None, None);
    let mut left = Vec::new();
    let mut it = args.iter().cloned();
    while let Some(arg) = it.next() {
        if !found.consume(&arg, &mut it).map_err(|e| format!("{command}: {e}"))? {
            left.push(arg);
        }
    }
    let roots = wordcraft_engine::file_access::AutomationRoots::new(found.read_root.as_deref(), found.write_root.as_deref())
        .map_err(|e| format!("{command}: {e}"))?;
    Ok((roots, left))
}

const NO_OPTIONS: &[OptionSpec] = &[];
const CONVERT_OPTIONS: &[OptionSpec] = &[OptionSpec { name: "--markup", takes_value: false }];
const RENDER_OPTIONS: &[OptionSpec] = &[OptionSpec { name: "--page", takes_value: true }, OptionSpec { name: "--scale", takes_value: true }];
const RUN_OPTIONS: &[OptionSpec] = &[
    OptionSpec { name: "--file", takes_value: true },
    OptionSpec { name: "--template", takes_value: true },
    OptionSpec { name: "--cmd", takes_value: true },
    OptionSpec { name: "--save", takes_value: true },
    OptionSpec { name: "--page", takes_value: true },
    OptionSpec { name: "--scale", takes_value: true },
    OptionSpec { name: "--print", takes_value: false },
    OptionSpec { name: "--author", takes_value: true },
];
const COMMANDS_OPTIONS: &[OptionSpec] = &[OptionSpec { name: "--json", takes_value: false }];
const PARITY_OPTIONS: &[OptionSpec] = &[OptionSpec { name: "--markdown", takes_value: false }];
const MCP_OPTIONS: &[OptionSpec] = &[OptionSpec { name: "--connect", takes_value: true }, OptionSpec { name: "--author", takes_value: true }];
const ZOTERO_OPTIONS: &[OptionSpec] = &[
    OptionSpec { name: "--cmd", takes_value: true },
    OptionSpec { name: "--save", takes_value: true },
    OptionSpec { name: "--trace", takes_value: false },
    OptionSpec { name: "--port", takes_value: true },
];

/// Why a CLI run stopped early.
#[derive(Debug, PartialEq)]
enum Failure {
    /// A real error, reported on stderr with a failing exit status.
    Error(String),
    /// Whoever read stdout closed it (`wordcraft-cli commands --json | head`): there is nobody left
    /// to write to, so stop quietly and exit 0 like other command-line tools.
    StdoutClosed,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Failure::Error(e) => f.write_str(e),
            Failure::StdoutClosed => f.write_str("stdout closed"),
        }
    }
}

impl From<String> for Failure {
    fn from(e: String) -> Self {
        Failure::Error(e)
    }
}

impl From<&str> for Failure {
    fn from(e: &str) -> Self {
        Failure::Error(e.into())
    }
}

impl From<std::io::Error> for Failure {
    fn from(e: std::io::Error) -> Self {
        if e.kind() == std::io::ErrorKind::BrokenPipe { Failure::StdoutClosed } else { Failure::Error(e.to_string()) }
    }
}

/// Write to stdout through one locked handle and flush. Unlike `println!`, a closed stdout comes
/// back as an error (`Failure::StdoutClosed`) instead of a panic.
fn write_stdout(write: impl FnOnce(&mut std::io::StdoutLock<'static>) -> std::io::Result<()>) -> Result<(), Failure> {
    let mut out = std::io::stdout().lock();
    write(&mut out)?;
    out.flush()?;
    Ok(())
}

fn run(args: &[String]) -> Result<(), Failure> {
    let cmd = args.first().map(String::as_str).unwrap_or("");
    let rest: Vec<String> = args.iter().skip(1).cloned().collect();
    let pos = |i: usize| rest.iter().filter(|a| !a.starts_with("--")).nth(i).cloned().ok_or_else(|| format!("missing argument\n\n{USAGE}"));
    match cmd {
        "convert" => {
            validate_options("convert", &rest, CONVERT_OPTIONS)?;
            let (i, o) = (pos(0)?, pos(1)?);
            let markup = rest.iter().any(|a| a == "--markup");
            let mut s = open(&i)?;
            let r = if o.to_ascii_lowercase().ends_with(".png") {
                s.run("file.exportPng", &json!({"path": o, "page": 1, "markup": markup}))
            } else {
                s.run("file.save", &json!({"path": o, "markup": markup}))
            };
            r.map_err(|e| e.to_string())?;
            eprintln!("wrote {o}");
            Ok(())
        }
        "info" => {
            validate_options("info", &rest, NO_OPTIONS)?;
            let mut s = open(&pos(0)?)?;
            let info = serde_json::to_string_pretty(&s.run("file.info", &json!({})).map_err(|e| e.to_string())?).unwrap_or_default();
            write_stdout(|o| writeln!(o, "{info}"))
        }
        "text" => {
            validate_options("text", &rest, NO_OPTIONS)?;
            let s = open(&pos(0)?)?;
            write_stdout(|o| writeln!(o, "{}", s.doc.plain_text(wordcraft_doc::StoryRef::Body)))
        }
        "inspect" => {
            validate_options("inspect", &rest, NO_OPTIONS)?;
            let mut s = open(&pos(0)?)?;
            let tree = serde_json::to_string_pretty(&s.run("document.inspect", &json!({})).map_err(|e| e.to_string())?).unwrap_or_default();
            write_stdout(|o| writeln!(o, "{tree}"))
        }
        "render" => {
            validate_options("render", &rest, RENDER_OPTIONS)?;
            let (i, o) = (pos(0)?, pos(1)?);
            let mut s = open(&i)?;
            let page = arg_value(&rest, "--page").and_then(|p| p.parse::<u64>().ok()).unwrap_or(1);
            let scale = arg_value(&rest, "--scale").and_then(|p| p.parse::<f64>().ok()).unwrap_or(2.0);
            let r = s.run("file.exportPng", &json!({"path": o, "page": page, "scale": scale})).map_err(|e| e.to_string())?;
            write_stdout(|o| writeln!(o, "{r}"))
        }
        "run" => {
            validate_options("run", &rest, RUN_OPTIONS)?;
            let mut s = match (arg_value(&rest, "--file"), arg_value(&rest, "--template")) {
                (Some(f), _) => open(&f)?,
                (None, Some(t)) => {
                    let mut s = Session::new(wordcraft_doc::Document::new());
                    s.run("file.new", &json!({"template": t})).map_err(|e| e.to_string())?;
                    s
                }
                _ => Session::new(wordcraft_doc::Document::new()),
            };
            if let Some(name) = arg_value(&rest, "--author") {
                set_author(&mut s, &name)?;
            }
            let mut i = 0;
            while i < rest.len() {
                if rest.get(i).map(String::as_str) == Some("--cmd") {
                    let spec = rest.get(i + 1).ok_or("--cmd needs a value")?;
                    let (id, params) = match spec.split_once('=') {
                        Some((id, p)) => (id.to_string(), serde_json::from_str::<Value>(p).map_err(|e| format!("{id}: bad JSON params: {e}"))?),
                        None => (spec.clone(), json!({})),
                    };
                    let r = s.run(&id, &params).map_err(|e| format!("{id}: {e}"))?;
                    if rest.iter().any(|a| a == "--print") {
                        write_stdout(|o| writeln!(o, "{id}: {r}"))?;
                    }
                    i += 2;
                } else {
                    i += 1;
                }
            }
            if let Some(out) = arg_value(&rest, "--save") {
                let r = if out.to_ascii_lowercase().ends_with(".png") {
                    let mut v = json!({"path": out});
                    if let Some(n) = arg_value(&rest, "--page").and_then(|p| p.parse::<u64>().ok()) {
                        v["page"] = json!(n);
                    }
                    if let Some(k) = arg_value(&rest, "--scale").and_then(|p| p.parse::<f64>().ok()) {
                        v["scale"] = json!(k);
                    }
                    s.run("file.exportPng", &v)
                } else {
                    s.run("file.save", &json!({"path": out}))
                };
                r.map_err(|e| e.to_string())?;
                eprintln!("wrote {out}");
            }
            Ok(())
        }
        "commands" => {
            validate_options("commands", &rest, COMMANDS_OPTIONS)?;
            let s = Session::new(wordcraft_doc::Document::new());
            if rest.iter().any(|a| a == "--json") {
                let list = serde_json::to_string_pretty(&s.registry.describe()).unwrap_or_default();
                write_stdout(|o| writeln!(o, "{list}"))
            } else {
                write_stdout(|o| {
                    for c in s.registry.all() {
                        writeln!(o, "{:<32} {:<36} {:<24} {}", c.id, c.label, c.shortcut, c.location)?;
                    }
                    Ok(())
                })
            }
        }
        "parity" => {
            validate_options("parity", &rest, PARITY_OPTIONS)?;
            let s = Session::new(wordcraft_doc::Document::new());
            let p = wordcraft_engine::catalog::parity(&s.registry);
            if rest.iter().any(|a| a == "--markdown") {
                write_stdout(|o| write!(o, "{}", parity_markdown(&p)))
            } else {
                write_stdout(|o| writeln!(o, "{}", serde_json::to_string_pretty(&p).unwrap_or_default()))
            }
        }
        "mcp" => {
            let (roots, rest) = automation_roots("mcp", &rest)?;
            validate_options("mcp", &rest, MCP_OPTIONS)?;
            let author = arg_value(&rest, "--author");
            let connect = arg_value(&rest, "--connect");
            if roots.is_some() && connect.is_some() {
                return Err(
                    "mcp: --automation-read-root and --automation-write-root confine the headless server; a running app is not confined by them"
                        .into(),
                );
            }
            let backend: Box<dyn wordcraft_mcp::Backend> = match connect {
                // The app's user name is the person's own setting; don't rewrite it from a flag.
                Some(_) if author.is_some() => {
                    return Err("mcp: --author applies to the headless server; with --connect, run file.setAuthor".into());
                }
                Some(addr) => {
                    let remote = wordcraft_mcp::Remote::connect(&addr).map_err(|e| format!("can't reach the WordCraft app at {addr}: {e}"))?;
                    if let Some(w) = remote.key_warning() {
                        eprintln!("wordcraft-cli: {w}");
                    }
                    Box::new(remote)
                }
                None => {
                    let mut headless = wordcraft_mcp::Headless::default();
                    if let Some(roots) = roots {
                        // Every thread of this process, not only the server's: nothing reaches past the roots.
                        wordcraft_engine::file_access::confine_process(roots.clone());
                        let show = |root: Option<&std::path::Path>| root.map_or_else(|| "none".to_string(), |r| r.display().to_string());
                        eprintln!("wordcraft-cli: files confined: read root {}, write root {}", show(roots.read_root()), show(roots.write_root()));
                        headless = headless.with_automation_roots(Some(roots));
                    }
                    if let Some(name) = author {
                        set_author(&mut headless.session, &name)?;
                    }
                    Box::new(headless)
                }
            };
            let mut server = wordcraft_mcp::Server::new(backend);
            let stdin = std::io::stdin();
            // A client that closes our stdout has hung up: `serve` returns BrokenPipe and we end cleanly.
            Ok(server.serve(stdin.lock(), std::io::stdout())?)
        }
        "zotero" => {
            validate_options("zotero", &rest, ZOTERO_OPTIONS)?;
            let name = pos(0)?;
            let command = wordcraft_zotero::Command::from_name(&name).ok_or_else(|| format!("unknown Zotero command `{name}`\n\n{USAGE}"))?;
            let file = pos(1)?;
            let mut s = open(&file)?;
            for spec in rest.iter().zip(rest.iter().skip(1)).filter(|(a, _)| *a == "--cmd").map(|(_, v)| v) {
                let (id, params) = match spec.split_once('=') {
                    Some((id, p)) => (id.to_string(), serde_json::from_str::<Value>(p).map_err(|e| format!("{id}: bad JSON params: {e}"))?),
                    None => (spec.clone(), json!({})),
                };
                s.run(&id, &params).map_err(|e| format!("{id}: {e}"))?;
            }
            let mut opts = wordcraft_zotero::client::Options::default();
            if let Some(p) = arg_value(&rest, "--port") {
                opts.addr.set_port(p.parse().map_err(|_| format!("bad port `{p}`"))?);
            }
            let trace = rest.iter().any(|a| a == "--trace");
            let mut bridge = wordcraft_zotero::Bridge::new();
            let mut host = wordcraft_zotero::Headless::default();
            let out = wordcraft_zotero::client::run_command(&opts, command, &mut |call| {
                let r = bridge.handle(&mut s, &mut host, call);
                if trace {
                    eprintln!("← {} {}", call.method, clip(&Value::Array(call.args.clone()).to_string()));
                    eprintln!("→ {}", clip(&String::from_utf8_lossy(&wordcraft_zotero::wire::encode_reply(&r))));
                }
                r
            })
            .map_err(|e| e.to_string())?;
            for a in &host.alerts {
                eprintln!("Zotero: {a}");
            }
            eprintln!("{} calls, {}", out.calls, if out.completed { "completed" } else { "ended without completing (cancelled?)" });
            if let Some(o) = arg_value(&rest, "--save") {
                s.run("file.save", &json!({"path": o})).map_err(|e| e.to_string())?;
                eprintln!("wrote {o}");
            }
            Ok(())
        }
        "--version" | "-V" => write_stdout(|o| writeln!(o, "wordcraft-cli {}", env!("CARGO_PKG_VERSION"))),
        "" | "help" | "--help" | "-h" => write_stdout(|o| write!(o, "{USAGE}")),
        other => Err(format!("unknown command `{other}`\n\n{USAGE}").into()),
    }
}

/// At most 400 characters of `s`, for traces.
fn clip(s: &str) -> String {
    match s.char_indices().nth(400) {
        Some((i, _)) => format!("{}…", s.get(..i).unwrap_or(s)),
        None => s.to_string(),
    }
}

fn parity_markdown(p: &Value) -> String {
    let mut s = String::from(
        "# WordCraft feature parity\n\nGenerated by `cargo xtask parity` (`wordcraft-cli parity --markdown`): the word-processor feature catalog (`crates/engine/src/catalog.rs`) compared with the live command registry.\n\n",
    );
    s += &format!(
        "**{} of {} catalog features have commands ({}%).**\n\n| Tab | Live | Total |\n|---|---|---|\n",
        p["live"], p["total"], p["percent"]
    );
    for t in p["tabs"].as_array().cloned().unwrap_or_default() {
        s += &format!("| {} | {} | {} |\n", t["tab"].as_str().unwrap_or(""), t["live"], t["total"]);
    }
    s += "\n## Missing\n\n";
    for t in p["tabs"].as_array().cloned().unwrap_or_default() {
        let m = t["missing"].as_array().cloned().unwrap_or_default();
        if m.is_empty() {
            continue;
        }
        s += &format!("### {}\n\n", t["tab"].as_str().unwrap_or(""));
        for x in m {
            s += &format!("- {}\n", x.as_str().unwrap_or(""));
        }
        s += "\n";
    }
    s
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) | Err(Failure::StdoutClosed) => ExitCode::SUCCESS,
        Err(Failure::Error(e)) => {
            eprintln!("wordcraft-cli: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).into()).collect()
    }

    #[test]
    fn validators_accept_only_the_documented_options() {
        assert!(validate_options("info", &args(&["file.docx"]), NO_OPTIONS).is_ok());
        assert!(validate_options("render", &args(&["in.docx", "out.png", "--page", "2", "--scale", "1.5"]), RENDER_OPTIONS).is_ok());
        assert!(validate_options("run", &args(&["--cmd", "text.insert={}", "--cmd", "edit.undo", "--print"]), RUN_OPTIONS).is_ok());
        assert!(validate_options("commands", &args(&["--json"]), COMMANDS_OPTIONS).is_ok());
        assert!(validate_options("parity", &args(&["--markdown"]), PARITY_OPTIONS).is_ok());
        assert!(validate_options("mcp", &args(&["--connect", "127.0.0.1:9000"]), MCP_OPTIONS).is_ok());
        assert!(validate_options("render", &args(&["in.docx", "out.png", "--page"]), RENDER_OPTIONS).is_ok());
        assert!(
            validate_options(
                "zotero",
                &args(&["refresh", "in.docx", "--cmd", "caret.docEnd", "--save", "out.docx", "--trace", "--port", "23119"]),
                ZOTERO_OPTIONS
            )
            .is_ok()
        );
    }

    #[test]
    fn unknown_options_are_rejected_before_file_access() {
        assert_eq!(run(&args(&["info", "missing.docx", "--passwrod", "x"])), Err("info: unknown option --passwrod".into()));
        assert_eq!(run(&args(&["render", "missing.docx", "out.png", "--sclae", "2"])), Err("render: unknown option --sclae".into()));
        assert_eq!(run(&args(&["zotero", "refresh", "missing.docx", "--prot", "1"])), Err("zotero: unknown option --prot".into()));
    }

    #[test]
    fn author_flag_names_the_headless_session() {
        assert!(validate_options("run", &args(&["--author", "Claude (copyedit)", "--cmd", "edit.undo"]), RUN_OPTIONS).is_ok());
        assert!(validate_options("mcp", &args(&["--author", "Claude"]), MCP_OPTIONS).is_ok());
        let mut s = Session::new(wordcraft_doc::Document::new());
        assert!(set_author(&mut s, " Claude (copyedit) ").is_ok());
        assert_eq!(s.author, "Claude (copyedit)");
        assert!(set_author(&mut s, "   ").is_err_and(|e| e.starts_with("--author:")));
        assert!(run(&args(&["run", "--author", "Claude", "--cmd", "review.trackChanges={\"value\":true}"])).is_ok());
        // The app's user name is the person's own; a bridge flag must not rewrite it.
        assert!(run(&args(&["mcp", "--connect", "127.0.0.1:9", "--author", "Claude"])).is_err_and(|e| e.to_string().contains("--author")));
    }

    #[test]
    fn automation_root_flags_are_taken_out_of_the_mcp_arguments() {
        let base = std::env::temp_dir().join(format!("wc-cli-roots-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("in")).unwrap();
        std::fs::create_dir_all(base.join("out")).unwrap();
        let (read, write) = (base.join("in").to_string_lossy().into_owned(), base.join("out").to_string_lossy().into_owned());

        let (roots, left) =
            automation_roots("mcp", &args(&["--automation-read-root", &read, "--author", "Claude", &format!("--automation-write-root={write}")]))
                .unwrap();
        let roots = roots.unwrap();
        assert_eq!(left, args(&["--author", "Claude"]));
        assert!(roots.read_root().is_some() && roots.write_root().is_some());
        assert!(validate_options("mcp", &left, MCP_OPTIONS).is_ok());

        // A root left out grants none of its access.
        let (roots, _) = automation_roots("mcp", &args(&["--automation-read-root", &read])).unwrap();
        let roots = roots.unwrap();
        assert!(roots.write_root().is_none());
        assert!(
            roots.check(&format!("{write}/x.docx"), wordcraft_engine::file_access::Access::Write).unwrap_err().contains("write authority is absent")
        );

        // No flags: nothing is confined and the arguments pass through.
        let (roots, left) = automation_roots("mcp", &args(&["--connect", "127.0.0.1:9"])).unwrap();
        assert!(roots.is_none());
        assert_eq!(left, args(&["--connect", "127.0.0.1:9"]));
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn automation_root_flags_reject_bad_operands_and_missing_folders() {
        for bad in [
            args(&["--automation-read-root"]),
            args(&["--automation-read-root", "--author"]),
            args(&["--automation-write-root="]),
            args(&["--automation-read-root", "a", "--automation-read-root=b"]),
        ] {
            assert!(automation_roots("mcp", &bad).is_err(), "{bad:?}");
        }
        let e = automation_roots("mcp", &args(&["--automation-read-root", "/definitely/not/a/folder"])).err().unwrap();
        assert!(e.starts_with("mcp: cannot open the automation read root"), "{e}");
        // Through the command line: an error, not a server that quietly runs unconfined.
        assert!(run(&args(&["mcp", "--automation-write-root", "/definitely/not/a/folder"])).is_err());
    }

    #[test]
    fn automation_roots_are_an_error_with_connect() {
        let dir = std::env::temp_dir().to_string_lossy().into_owned();
        let e = run(&args(&["mcp", "--connect", "127.0.0.1:9", "--automation-read-root", &dir])).unwrap_err().to_string();
        assert!(e.contains("--automation-read-root") && e.contains("headless"), "{e}");
    }
}
