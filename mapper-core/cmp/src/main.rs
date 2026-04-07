mod api;
mod formatter;
mod layers;
mod mapper;
mod mcp;
mod memory;
mod scanner;
mod sync;
mod uc_agents;
mod uc_analytics;
mod uc_client;
mod uc_sync;
mod uc_webhooks;
mod webhooks;

use anyhow::{Context, Result};
use arboard::Clipboard;
use clap::{Parser, Subcommand, ValueEnum};
use formatter::{estimate_tokens, format_token_count, get_formatter, OutputTarget};
use mapper::{extract_skeleton, MappedFile};
use memory::Memory;
use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode, DebouncedEventKind};
use scanner::{is_ignored_path, scan_files_with_noise_tracking, IgnoredFile};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::time::Duration;
use sync::SyncService;
use uc_agents::{AgentService, AgentType};
use uc_analytics::AnalyticsService;
use uc_sync::UCSyncService;
use uc_webhooks::{AgentContext, WebhookService};

const TOKEN_THRESHOLD_GREEN: usize = 10_000;
const TOKEN_THRESHOLD_YELLOW: usize = 30_000;
const WATCH_DEBOUNCE_MS: u64 = 500;

#[derive(Parser)]
#[command(name = "cmp")]
#[command(about = "Memory Unit - Deterministic codebase mapper for AI context injection")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Target folder to scan (defaults to current directory)
    #[arg(value_name = "PATH")]
    path: Option<PathBuf>,

    #[arg(short, long, default_value = "claude")]
    target: Target,

    #[arg(short, long)]
    copy: bool,

    #[arg(long = "ignore", value_name = "FILE")]
    ignore_files: Vec<String>,
}

#[derive(Clone, ValueEnum, Default)]
enum Target {
    Raw,
    #[default]
    Claude,
    Cursor,
}

impl From<Target> for OutputTarget {
    fn from(t: Target) -> Self {
        match t {
            Target::Raw => OutputTarget::Raw,
            Target::Claude => OutputTarget::Claude,
            Target::Cursor => OutputTarget::Cursor,
        }
    }
}

#[derive(Subcommand)]
enum Commands {
    /// Mode A: Skeleton map (imports + signatures only)
    Map {
        #[arg(value_name = "PATH")]
        path: Option<PathBuf>,
    },
    /// Mode B: Full source code (saves to disk)
    Source {
        #[arg(value_name = "PATH")]
        path: Option<PathBuf>,
    },
    /// Live watcher - keeps skeleton map updated, NO full source to disk
    Watch {
        #[arg(value_name = "PATH")]
        path: Option<PathBuf>,
    },
    /// Copy full source to clipboard (ephemeral - no disk write)
    Copy {
        #[arg(value_name = "PATH")]
        path: Option<PathBuf>,
    },
    /// Incremental sync
    Sync {
        #[arg(value_name = "PATH")]
        path: Option<PathBuf>,
    },
    /// Initialize UC cloud sync
    Init {
        #[arg(long)]
        cloud: bool,
        #[arg(long, value_name = "NAME")]
        project: Option<String>,
    },
    /// Push local context to UC
    Push,
    /// Pull context from UC
    Pull {
        #[arg(long, value_name = "VERSION")]
        version: Option<u32>,
    },
    /// Show UC context history
    History,
    /// Create a context branch
    Branch {
        #[arg(value_name = "NAME")]
        name: String,
        #[arg(long, value_name = "VERSION")]
        from: Option<u32>,
    },
    /// Diff between two versions
    Diff {
        #[arg(value_name = "V1")]
        v1: u32,
        #[arg(value_name = "V2")]
        v2: u32,
    },
    /// Manage AI agents
    Agents {
        #[command(subcommand)]
        command: AgentCommands,
    },
    /// View analytics dashboard
    Analytics,
    /// Get optimization suggestions
    Optimize,
    /// Export context for agents
    Export {
        #[arg(value_name = "FORMAT", default_value = "json")]
        format: String,
        #[arg(short = 'o', long, value_name = "FILE")]
        output: Option<PathBuf>,
    },
    /// Notify agents of context update
    Notify,
    /// Initialize Cartographer with CKB integration
    InitCkb {
        #[arg(long, value_name = "CKB_URL")]
        ckb_url: Option<String>,
        #[arg(long, value_name = "WEBHOOK_URL")]
        webhook_url: Option<String>,
    },
    /// Health check - shows architectural health score
    Health,
    /// Simulate how a change will impact the architecture
    Simulate {
        #[arg(long, value_name = "MODULE")]
        module: String,
        #[arg(long, value_name = "SIGNATURE")]
        new_signature: Option<String>,
        #[arg(long, value_name = "REMOVE")]
        remove_signature: Option<String>,
    },
    /// Show architecture evolution over time
    Evolution {
        #[arg(long, value_name = "DAYS")]
        days: Option<u32>,
    },
}

#[derive(Subcommand)]
enum AgentCommands {
    /// List all configured agents
    List,
    /// Add a new agent
    Add {
        #[arg(value_name = "NAME")]
        name: String,
        #[arg(short = 't', long = "type", value_name = "TYPE")]
        agent_type: String,
        #[arg(long, value_name = "KEY")]
        api_key: Option<String>,
        #[arg(long, value_name = "URL")]
        webhook: Option<String>,
    },
    /// Remove an agent
    Remove {
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Show agent details
    Show {
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Enable an agent
    Enable {
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Disable an agent
    Disable {
        #[arg(value_name = "ID")]
        id: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let cwd = std::env::current_dir().context("Failed to get current directory")?;
    let target: OutputTarget = cli.target.into();
    let ignore_set: HashSet<String> = cli.ignore_files.into_iter().collect();

    match cli.command {
        Some(Commands::Map { path }) => {
            let root = resolve_path(&cwd, path.or(cli.path.clone()))?;
            map_mode(&root, &cwd, target, cli.copy)
        }
        Some(Commands::Source { path }) => {
            let root = resolve_path(&cwd, path.or(cli.path.clone()))?;
            source_mode(&root, &cwd, target, cli.copy, &ignore_set)
        }
        Some(Commands::Watch { path }) => {
            let root = resolve_path(&cwd, path.or(cli.path.clone()))?;
            live_watch_mode(&root, &cwd, target)
        }
        Some(Commands::Copy { path }) => {
            let root = resolve_path(&cwd, path.or(cli.path.clone()))?;
            copy_mode(&root, target, &ignore_set)
        }
        Some(Commands::Sync { path }) => {
            let root = resolve_path(&cwd, path.or(cli.path.clone()))?;
            sync_mode(&root, &cwd, target, cli.copy)
        }
        Some(Commands::Init { cloud, project }) => {
            let root = resolve_path(&cwd, cli.path)?;
            if cloud {
                init_cloud_mode(&root, project.as_deref())
            } else {
                println!("Use --cloud flag to initialize UC sync");
                Ok(())
            }
        }
        Some(Commands::Push) => {
            let root = resolve_path(&cwd, cli.path)?;
            push_mode(&root)
        }
        Some(Commands::Pull { version }) => {
            let root = resolve_path(&cwd, cli.path)?;
            pull_mode(&root, version)
        }
        Some(Commands::History) => {
            let root = resolve_path(&cwd, cli.path)?;
            history_mode(&root)
        }
        Some(Commands::Branch { name, from }) => {
            let root = resolve_path(&cwd, cli.path)?;
            branch_mode(&root, &name, from)
        }
        Some(Commands::Diff { v1, v2 }) => {
            let root = resolve_path(&cwd, cli.path)?;
            diff_mode(&root, v1, v2)
        }
        Some(Commands::Agents { command }) => {
            let root = resolve_path(&cwd, cli.path)?;
            agents_mode(&root, command)
        }
        Some(Commands::Analytics) => {
            let root = resolve_path(&cwd, cli.path)?;
            analytics_mode(&root)
        }
        Some(Commands::Optimize) => {
            let root = resolve_path(&cwd, cli.path)?;
            optimize_mode(&root)
        }
        Some(Commands::Export { format, output }) => {
            let root = resolve_path(&cwd, cli.path)?;
            export_mode(&root, &format, output.as_deref())
        }
        Some(Commands::Notify) => {
            let root = resolve_path(&cwd, cli.path)?;
            notify_mode(&root)
        }
        Some(Commands::InitCkb {
            ckb_url,
            webhook_url,
        }) => {
            let root = resolve_path(&cwd, cli.path)?;
            init_ckb_mode(&root, ckb_url.as_deref(), webhook_url.as_deref())
        }
        Some(Commands::Health) => {
            let root = resolve_path(&cwd, cli.path)?;
            health_mode(&root)
        }
        Some(Commands::Simulate {
            module,
            new_signature,
            remove_signature,
        }) => {
            let root = resolve_path(&cwd, cli.path)?;
            simulate_mode(
                &root,
                &module,
                new_signature.as_deref(),
                remove_signature.as_deref(),
            )
        }
        Some(Commands::Evolution { days }) => {
            let root = resolve_path(&cwd, cli.path)?;
            evolution_mode(&root, days)
        }
        None => {
            let root = resolve_path(&cwd, cli.path)?;
            source_mode(&root, &cwd, target, cli.copy, &ignore_set)
        }
    }
}

fn resolve_path(cwd: &Path, path: Option<PathBuf>) -> Result<PathBuf> {
    match path {
        Some(p) => {
            let resolved = if p.is_absolute() { p } else { cwd.join(&p) };
            if !resolved.exists() {
                anyhow::bail!("Path does not exist: {}", resolved.display());
            }
            if !resolved.is_dir() {
                anyhow::bail!("Path is not a directory: {}", resolved.display());
            }
            Ok(resolved)
        }
        None => Ok(cwd.to_path_buf()),
    }
}

// =============================================================================
// LIVE WATCH MODE - Lightweight skeleton map only, NO full source to disk
// =============================================================================

fn live_watch_mode(root: &Path, output_dir: &Path, target: OutputTarget) -> Result<()> {
    println!("LIVE WATCHER: Monitoring {}...", root.display());
    println!("============================================");
    println!("  Mode: Skeleton Map ONLY (lightweight)");
    println!("  Debounce: {}ms", WATCH_DEBOUNCE_MS);
    println!("  Full source: Use 'cmp copy' when needed");
    println!("============================================");
    println!("Press Ctrl+C to stop\n");

    // Initial skeleton map generation
    let (mapped_files, ignored) = generate_skeleton_map(root)?;
    let output = format_map_output(&mapped_files, target);
    let tokens = estimate_tokens(&output);

    // Write lightweight map file
    let formatter = get_formatter(target);
    let map_filename = format!("cmp_map.{}", formatter.extension());
    let map_path = output_dir.join(&map_filename);
    fs::write(&map_path, &output)?;

    print_cmp_report(mapped_files.len(), &ignored);
    println!("Map: {} | {}", map_filename, format_token_count(tokens));
    println!("Watching for changes...\n");

    // Setup file watcher with 500ms debounce
    let (tx, rx) = channel();
    let mut debouncer = new_debouncer(Duration::from_millis(WATCH_DEBOUNCE_MS), tx)?;
    debouncer.watcher().watch(root, RecursiveMode::Recursive)?;

    loop {
        match rx.recv() {
            Ok(Ok(events)) => {
                // Filter out irrelevant events (our own output, ignored paths)
                let relevant = events.iter().any(|e| {
                    e.kind == DebouncedEventKind::Any
                        && !e.path.ends_with(&map_filename)
                        && !e.path.ends_with(".cmp_memory.json")
                        && !e.path.ends_with("context.xml")
                        && !e.path.ends_with("context.md")
                        && !e.path.ends_with("context.json")
                        && !is_ignored_path(&e.path)
                });

                if relevant {
                    // Regenerate skeleton map only
                    match generate_skeleton_map(root) {
                        Ok((files, _)) => {
                            let output = format_map_output(&files, target);
                            let tokens = estimate_tokens(&output);
                            if fs::write(&map_path, &output).is_ok() {
                                println!(
                                    "[{}] Map updated: {} files, {}",
                                    chrono_time(),
                                    files.len(),
                                    format_token_count(tokens)
                                );
                            }
                        }
                        Err(e) => eprintln!("Error updating map: {}", e),
                    }
                }
            }
            Ok(Err(e)) => eprintln!("Watch error: {:?}", e),
            Err(e) => {
                eprintln!("Channel error: {}", e);
                break;
            }
        }
    }
    Ok(())
}

fn generate_skeleton_map(root: &Path) -> Result<(Vec<MappedFile>, Vec<IgnoredFile>)> {
    let scan_result = scan_files_with_noise_tracking(root)?;
    let mut mapped_files: Vec<MappedFile> = Vec::new();

    for path in &scan_result.files {
        if let Some(content) = read_text_file(path) {
            let rel_path = path.strip_prefix(root).unwrap_or(path);
            let skeleton = extract_skeleton(rel_path, &content);
            if !skeleton.imports.is_empty() || !skeleton.signatures.is_empty() {
                mapped_files.push(skeleton);
            }
        }
    }

    Ok((mapped_files, scan_result.ignored_noise))
}

fn chrono_time() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs() % 86400;
    let hours = (secs / 3600) % 24;
    let mins = (secs % 3600) / 60;
    let secs = secs % 60;
    format!("{:02}:{:02}:{:02}", hours, mins, secs)
}

// =============================================================================
// COPY MODE - Ephemeral full source to clipboard (NO disk write)
// =============================================================================

fn copy_mode(root: &Path, target: OutputTarget, ignore_set: &HashSet<String>) -> Result<()> {
    println!("COPY MODE: Generating full source (ephemeral)...");

    let service = SyncService::new(root);
    let result = service.full_scan_with_noise()?;
    let mut memory = result.memory;
    let ignored = result.ignored_noise;

    // Apply user ignores
    if !ignore_set.is_empty() {
        memory.files.retain(|path, _| {
            let filename = path.rsplit('/').next().unwrap_or(path);
            !ignore_set.contains(filename) && !ignore_set.contains(path)
        });
    }

    print_cmp_report(memory.files.len(), &ignored);

    // Generate output to memory only (NOT to disk)
    let formatter = get_formatter(target);
    let output = formatter.format(&memory);
    let tokens = estimate_tokens(&output);

    println!(
        "Generated: {} files, {}",
        memory.files.len(),
        format_token_count(tokens)
    );

    // Token budget check then copy to clipboard
    if tokens > TOKEN_THRESHOLD_YELLOW {
        println!("\nHIGH COST WARNING");
        println!(
            "Token count: {} | Estimated cost: ~${:.2}",
            format_token_count(tokens),
            estimate_cost(tokens)
        );
        println!("Recommend using `cmp map` first or targeting a specific folder.\n");
        print!("[?] Copy to clipboard anyway? (y/N) ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if input.trim().eq_ignore_ascii_case("y") || input.trim().eq_ignore_ascii_case("yes") {
            copy_to_clipboard(&output)?;
        } else {
            println!("Cancelled. No data written to disk or clipboard.");
        }
    } else if tokens > TOKEN_THRESHOLD_GREEN {
        println!("\nMODERATE COST");
        println!(
            "Token count: {} | Estimated cost: ~${:.2}",
            format_token_count(tokens),
            estimate_cost(tokens)
        );
        print!("[?] Copy to clipboard? (Y/n) ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();
        if input.is_empty() || input.eq_ignore_ascii_case("y") || input.eq_ignore_ascii_case("yes")
        {
            copy_to_clipboard(&output)?;
        } else {
            println!("Cancelled. No data written to disk or clipboard.");
        }
    } else {
        // Green zone - copy directly
        copy_to_clipboard(&output)?;
    }

    Ok(())
}

// =============================================================================
// MAP MODE - One-shot skeleton map generation
// =============================================================================

fn map_mode(root: &Path, output_dir: &Path, target: OutputTarget, copy: bool) -> Result<()> {
    println!("MAP MODE: Scanning {}...", root.display());

    let (mapped_files, ignored) = generate_skeleton_map(root)?;
    print_cmp_report(mapped_files.len(), &ignored);

    let output = format_map_output(&mapped_files, target);
    let tokens = estimate_tokens(&output);

    let formatter = get_formatter(target);
    let filename = format!("cmp_map.{}", formatter.extension());
    fs::write(output_dir.join(&filename), &output)?;

    println!(
        "Generated: {} | {} files, {}",
        filename,
        mapped_files.len(),
        format_token_count(tokens)
    );
    handle_token_budget_copy(&output, tokens, copy)?;
    Ok(())
}

// =============================================================================
// SOURCE MODE - Full source to disk (legacy behavior)
// =============================================================================

fn source_mode(
    root: &Path,
    output_dir: &Path,
    target: OutputTarget,
    copy: bool,
    ignore_set: &HashSet<String>,
) -> Result<()> {
    println!("SOURCE MODE: Scanning {}...", root.display());

    let service = SyncService::new(root);
    let result = service.full_scan_with_noise()?;
    let mut memory = result.memory;
    let ignored = result.ignored_noise;

    if !ignore_set.is_empty() {
        memory.files.retain(|path, _| {
            let filename = path.rsplit('/').next().unwrap_or(path);
            !ignore_set.contains(filename) && !ignore_set.contains(path)
        });
        println!("User-ignored {} file(s)", ignore_set.len());
    }

    print_cmp_report(memory.files.len(), &ignored);
    let memory = handle_ignored_consent(&service, memory, &ignored)?;
    memory.save(output_dir)?;
    let output = write_output(output_dir, &memory, target)?;
    let tokens = estimate_tokens(&output);
    println!(
        "Generated context ({} files, {})",
        memory.files.len(),
        format_token_count(tokens)
    );
    handle_token_budget_copy(&output, tokens, copy)?;
    Ok(())
}

// =============================================================================
// SYNC MODE - Incremental update
// =============================================================================

fn sync_mode(root: &Path, output_dir: &Path, target: OutputTarget, copy: bool) -> Result<()> {
    println!("SYNC MODE: Scanning {}...", root.display());

    let service = SyncService::new(root);
    let existing = Memory::load(output_dir).unwrap_or_default();
    let result = service.incremental_sync_with_noise(existing)?;
    let memory = result.memory;
    let ignored = result.ignored_noise;

    print_cmp_report(memory.files.len(), &ignored);
    let memory = handle_ignored_consent(&service, memory, &ignored)?;
    memory.save(output_dir)?;
    let output = write_output(output_dir, &memory, target)?;
    let tokens = estimate_tokens(&output);
    println!(
        "Synced context ({} files, {})",
        memory.files.len(),
        format_token_count(tokens)
    );
    handle_token_budget_copy(&output, tokens, copy)?;
    Ok(())
}

// =============================================================================
// Formatting helpers
// =============================================================================

fn format_map_output(files: &[MappedFile], target: OutputTarget) -> String {
    match target {
        OutputTarget::Claude => format_map_xml(files),
        OutputTarget::Cursor => format_map_markdown(files),
        OutputTarget::Raw => format_map_json(files),
    }
}

fn format_map_xml(files: &[MappedFile]) -> String {
    let mut out = String::from("<context type=\"skeleton_map\">\n<project_map>\n");
    for file in files {
        out.push_str(&format!("<file path=\"{}\">\n", escape_xml(&file.path)));
        out.push_str(&escape_xml(&file.format()));
        out.push_str("</file>\n");
    }
    out.push_str("</project_map>\n</context>");
    out
}

fn format_map_markdown(files: &[MappedFile]) -> String {
    let mut out = String::from("# Project Skeleton Map\n\n");
    for file in files {
        let ext = file.path.rsplit('.').next().unwrap_or("txt");
        out.push_str(&format!(
            "## {}\n\n`{}\n{}\n`\n\n",
            file.path,
            ext,
            file.format()
        ));
    }
    out
}

fn format_map_json(files: &[MappedFile]) -> String {
    let json_files: Vec<_> = files.iter().map(|f| serde_json::json!({"path": f.path, "imports": f.imports, "signatures": f.signatures})).collect();
    serde_json::to_string_pretty(&json_files).unwrap_or_default()
}

// =============================================================================
// CMP Report
// =============================================================================

fn print_cmp_report(included_count: usize, ignored: &[IgnoredFile]) {
    println!();
    println!("CMP REPORT:");
    println!("============================================");
    println!("  Included: {} files (Source Code)", included_count);
    if ignored.is_empty() {
        println!("  Ignored Noise: None");
    } else {
        let noise_names: Vec<&str> = ignored
            .iter()
            .take(5)
            .map(|i| i.path.rsplit('/').next().unwrap_or(&i.path))
            .collect();
        let display = if ignored.len() > 5 {
            format!(
                "{}, ... (+{} more)",
                noise_names.join(", "),
                ignored.len() - 5
            )
        } else {
            noise_names.join(", ")
        };
        let total_tokens: usize = ignored.iter().map(|i| i.estimated_tokens).sum();
        println!(
            "  Ignored Noise: {} (saved ~{})",
            display,
            format_token_count(total_tokens)
        );
    }
    println!("============================================");
}

// =============================================================================
// Token Budget Check
// =============================================================================

fn handle_token_budget_copy(content: &str, tokens: usize, auto_copy: bool) -> Result<()> {
    if tokens > TOKEN_THRESHOLD_YELLOW {
        println!("\nHIGH COST WARNING");
        println!(
            "Token count: {} | Estimated cost: ~${:.2}",
            format_token_count(tokens),
            estimate_cost(tokens)
        );
        println!("Recommend using `cmp map` first or targeting a specific folder.\n");
        print!("[?] Proceed with copy? (y/N) ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if input.trim().eq_ignore_ascii_case("y") || input.trim().eq_ignore_ascii_case("yes") {
            copy_to_clipboard(content)?;
        } else {
            println!("Not copied (file still saved to disk)");
        }
    } else if tokens > TOKEN_THRESHOLD_GREEN {
        println!("\nMODERATE COST");
        println!(
            "Token count: {} | Estimated cost: ~${:.2}",
            format_token_count(tokens),
            estimate_cost(tokens)
        );
        print!("[?] Proceed with copy? (Y/n) ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();
        if input.is_empty() || input.eq_ignore_ascii_case("y") || input.eq_ignore_ascii_case("yes")
        {
            copy_to_clipboard(content)?;
        } else {
            println!("Not copied (file still saved to disk)");
        }
    } else {
        if auto_copy {
            copy_to_clipboard(content)?;
        } else {
            print!("[?] Copy to clipboard? (Y/n) ");
            io::stdout().flush()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            let input = input.trim();
            if input.is_empty()
                || input.eq_ignore_ascii_case("y")
                || input.eq_ignore_ascii_case("yes")
            {
                copy_to_clipboard(content)?;
            } else {
                println!("Saved to disk only");
            }
        }
    }
    Ok(())
}

fn estimate_cost(tokens: usize) -> f64 {
    (tokens as f64 / 1000.0) * 0.01
}

// =============================================================================
// Helper Functions
// =============================================================================

fn write_output(root: &Path, memory: &Memory, target: OutputTarget) -> Result<String> {
    let formatter = get_formatter(target);
    let output = formatter.format(memory);
    let filename = format!("context.{}", formatter.extension());
    let file = File::create(root.join(&filename))?;
    let mut writer = BufWriter::new(file);
    write!(writer, "{}", output)?;
    writer.flush()?;
    Ok(output)
}

fn copy_to_clipboard(content: &str) -> Result<()> {
    match Clipboard::new() {
        Ok(mut clipboard) => {
            clipboard
                .set_text(content.to_string())
                .context("Failed to copy to clipboard")?;
            println!("Copied to clipboard");
            Ok(())
        }
        Err(e) => {
            eprintln!("Clipboard unavailable: {}", e);
            Ok(())
        }
    }
}

fn read_text_file(path: &Path) -> Option<String> {
    let content = fs::read(path).ok()?;
    let check_len = content.len().min(8192);
    if content[..check_len].contains(&0) {
        return None;
    }
    String::from_utf8(content).ok()
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn handle_ignored_consent(
    service: &SyncService,
    mut memory: Memory,
    ignored: &[IgnoredFile],
) -> Result<Memory> {
    if ignored.is_empty() {
        return Ok(memory);
    }
    let total_tokens: usize = ignored.iter().map(|i| i.estimated_tokens).sum();
    print!(
        "\n[?] Force-include {} ignored files? (y/N) ",
        ignored.len()
    );
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    if input.trim().eq_ignore_ascii_case("y") || input.trim().eq_ignore_ascii_case("yes") {
        println!(
            "WARNING: Adding ~{} of noise!",
            format_token_count(total_tokens)
        );
        service.include_ignored_files(&mut memory, ignored);
        println!("Force-included {} files", ignored.len());
    } else {
        println!("Keeping noise files excluded (recommended)");
    }
    Ok(memory)
}

// =============================================================================
// UC CLOUD SYNC MODES
// =============================================================================

fn get_uc_api_key() -> Result<String> {
    // Try environment variable first
    if let Ok(key) = std::env::var("ULTRA_CONTEXT") {
        return Ok(key);
    }

    // Try .env.local in current directory
    if let Ok(content) = fs::read_to_string(".env.local") {
        for line in content.lines() {
            if line.starts_with("ULTRA_CONTEXT=") {
                if let Some(key) = line.strip_prefix("ULTRA_CONTEXT=") {
                    return Ok(key.trim().to_string());
                }
            }
        }
    }

    // Try .env.local in parent directory
    if let Ok(content) = fs::read_to_string("../.env.local") {
        for line in content.lines() {
            if line.starts_with("ULTRA_CONTEXT=") {
                if let Some(key) = line.strip_prefix("ULTRA_CONTEXT=") {
                    return Ok(key.trim().to_string());
                }
            }
        }
    }

    anyhow::bail!("UC API key not found. Set ULTRA_CONTEXT env var or add to .env.local")
}

fn init_cloud_mode(root: &Path, project_name: Option<&str>) -> Result<()> {
    let api_key = get_uc_api_key()?;
    let service = UCSyncService::new(api_key, root)?;

    let project = project_name.unwrap_or_else(|| {
        root.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("my-project")
    });

    service.init(project)?;
    Ok(())
}

fn push_mode(root: &Path) -> Result<()> {
    let api_key = get_uc_api_key()?;
    let service = UCSyncService::new(api_key, root)?;

    // Load local memory
    let memory = Memory::load(root).context("No local memory found. Run 'cmp source' first.")?;

    // Track what changed for webhook notification
    let old_config = uc_sync::UCConfig::load(root).ok();
    let old_files: HashSet<String> = old_config
        .as_ref()
        .map(|c| c.file_hashes.keys().cloned().collect())
        .unwrap_or_default();

    let new_files: HashSet<String> = memory.files.keys().cloned().collect();

    let added: Vec<String> = new_files.difference(&old_files).cloned().collect();
    let deleted: Vec<String> = old_files.difference(&new_files).cloned().collect();

    // Detect modified files by comparing hashes
    let mut modified: Vec<String> = Vec::new();
    if let Some(ref old_cfg) = old_config {
        for (path, entry) in &memory.files {
            if let Some(&old_hash) = old_cfg.file_hashes.get(path) {
                if old_hash != entry.hash {
                    modified.push(path.clone());
                }
            }
        }
    }

    // Push to UC
    let config = service.push(&memory)?;

    // Notify agents via webhooks
    let agent_service = AgentService::new(root);
    if let Ok(agents) = agent_service.list_agents() {
        if !agents.is_empty() {
            println!("\nNotifying {} agent(s)...", agents.len());

            let webhook_service = WebhookService::new()?;
            let payload = uc_webhooks::WebhookService::create_payload(
                &config.context_id,
                config.last_version,
                added.clone(),
                modified.clone(),
                deleted.clone(),
                memory.files.len(),
            );

            let results = webhook_service.notify_all(&agents, &payload);
            let success_count = results.iter().filter(|r| r.is_ok()).count();
            let fail_count = results.len() - success_count;

            if success_count > 0 {
                println!("✓ Notified {} agent(s)", success_count);
            }
            if fail_count > 0 {
                println!("⚠️  {} agent(s) failed to notify", fail_count);
                for (i, result) in results.iter().enumerate() {
                    if let Err(e) = result {
                        println!("  - Agent {}: {}", i + 1, e);
                    }
                }
            }
        }
    }

    Ok(())
}

fn pull_mode(root: &Path, version: Option<u32>) -> Result<()> {
    let api_key = get_uc_api_key()?;
    let service = UCSyncService::new(api_key, root)?;

    let memory = service.pull(version)?;
    memory.save(root)?;

    println!(
        "✓ Memory saved to {}",
        root.join(".cmp_memory.json").display()
    );
    Ok(())
}

fn history_mode(root: &Path) -> Result<()> {
    let api_key = get_uc_api_key()?;
    let service = UCSyncService::new(api_key, root)?;

    let history = service.history()?;

    if history.is_empty() {
        println!("No version history available.");
        return Ok(());
    }

    println!("\nContext Version History:");
    println!("============================================");
    for version in history {
        let affected = version
            .affected
            .as_ref()
            .map(|a: &Vec<String>| format!(" (affected: {})", a.len()))
            .unwrap_or_default();
        println!(
            "v{} - {} - {}{}",
            version.version, version.operation, version.timestamp, affected
        );
    }
    println!("============================================\n");

    Ok(())
}

fn branch_mode(root: &Path, name: &str, from_version: Option<u32>) -> Result<()> {
    let api_key = get_uc_api_key()?;
    let service = UCSyncService::new(api_key, root)?;

    service.branch(name, from_version)?;
    Ok(())
}

fn diff_mode(root: &Path, v1: u32, v2: u32) -> Result<()> {
    let api_key = get_uc_api_key()?;
    let service = UCSyncService::new(api_key, root)?;

    let diff = service.diff(v1, v2)?;
    diff.print();

    Ok(())
}

fn agents_mode(root: &Path, command: AgentCommands) -> Result<()> {
    let agent_service = AgentService::new(root);

    match command {
        AgentCommands::List => {
            agent_service.print_agents_table()?;
        }
        AgentCommands::Add {
            name,
            agent_type,
            api_key,
            webhook,
        } => {
            let config = uc_sync::UCConfig::load(root)?;

            let agent_type_enum = match agent_type.to_lowercase().as_str() {
                "cursor" => AgentType::Cursor,
                "copilot" => AgentType::Copilot,
                "claude" => AgentType::Claude,
                "custom" => AgentType::Custom,
                _ => anyhow::bail!("Invalid agent type. Use: cursor, copilot, claude, custom"),
            };

            agent_service.add_agent(
                &name,
                agent_type_enum,
                &config.context_id,
                api_key,
                webhook,
            )?;
        }
        AgentCommands::Remove { id } => {
            agent_service.remove_agent(&id)?;
        }
        AgentCommands::Show { id } => {
            agent_service.print_agent_details(&id)?;
        }
        AgentCommands::Enable { id } => {
            agent_service.enable_agent(&id)?;
        }
        AgentCommands::Disable { id } => {
            agent_service.disable_agent(&id)?;
        }
    }

    Ok(())
}

fn analytics_mode(root: &Path) -> Result<()> {
    let service = AnalyticsService::new(root);
    service.print_dashboard()?;
    Ok(())
}

fn optimize_mode(root: &Path) -> Result<()> {
    let service = AnalyticsService::new(root);
    let suggestions = service.optimize_suggestions()?;

    if suggestions.is_empty() {
        println!("✓ Context is already optimized!");
        return Ok(());
    }

    println!("\nOptimization Suggestions:");
    println!("============================================");
    for (i, suggestion) in suggestions.iter().enumerate() {
        println!("{}. {}", i + 1, suggestion);
    }
    println!("============================================\n");

    Ok(())
}

fn export_mode(root: &Path, format: &str, output: Option<&Path>) -> Result<()> {
    let memory = Memory::load(root).context("No local memory found. Run 'cmp source' first.")?;
    let config = uc_sync::UCConfig::load(root)?;

    let agent_context = AgentContext::from_memory(&memory, &config.context_id);

    let content = match format.to_lowercase().as_str() {
        "json" => agent_context.to_json()?,
        "markdown" | "md" => agent_context.to_markdown(),
        _ => anyhow::bail!("Unknown format: {}. Use 'json' or 'markdown'", format),
    };

    if let Some(output_path) = output {
        fs::write(output_path, &content)?;
        println!("✓ Exported to: {}", output_path.display());
    } else {
        println!("{}", content);
    }

    Ok(())
}

fn notify_mode(root: &Path) -> Result<()> {
    let memory = Memory::load(root).context("No local memory found. Run 'cmp source' first.")?;
    let config = uc_sync::UCConfig::load(root)?;
    let agent_service = AgentService::new(root);

    let agents = agent_service.list_agents()?;
    if agents.is_empty() {
        println!("No agents configured. Use 'cmp agents add' to add one.");
        return Ok(());
    }

    let webhook_agents: Vec<_> = agents.iter().filter(|a| a.webhook_url.is_some()).collect();
    if webhook_agents.is_empty() {
        println!("No agents with webhooks configured.");
        return Ok(());
    }

    println!(
        "Notifying {} agent(s) with webhooks...",
        webhook_agents.len()
    );

    let webhook_service = WebhookService::new()?;
    let payload = uc_webhooks::WebhookService::create_payload(
        &config.context_id,
        config.last_version,
        vec![],
        memory.files.keys().cloned().collect(),
        vec![],
        memory.files.len(),
    );

    let results = webhook_service.notify_all(&agents, &payload);
    let success_count = results.iter().filter(|r| r.is_ok()).count();
    let fail_count = results.len() - success_count;

    if success_count > 0 {
        println!("✓ Notified {} agent(s)", success_count);
    }
    if fail_count > 0 {
        println!("⚠️  {} agent(s) failed", fail_count);
        for result in results.iter() {
            if let Err(e) = result {
                println!("  - {}", e);
            }
        }
    }

    Ok(())
}

fn init_ckb_mode(root: &Path, ckb_url: Option<&str>, webhook_url: Option<&str>) -> Result<()> {
    println!("╔═══════════════════════════════════════════════════════════╗");
    println!("║      Cartographer v1.0.0 - CKB Integration Setup           ║");
    println!("╚═══════════════════════════════════════════════════════════╝");
    println!();

    let config_path = root.join(".cartographer").join("config.toml");
    let config_dir = config_path.parent().unwrap();
    std::fs::create_dir_all(config_dir)?;

    let ckb_url = ckb_url.unwrap_or("http://localhost:8080");
    let webhook_url = webhook_url.unwrap_or("http://localhost:8081/webhook");

    let config_content = format!(
        r#"# Cartographer Configuration
version = "1.0.0"

[ckb]
url = "{}"
enabled = true

[webhooks]
enabled = true
url = "{}"
events = ["graph_updated", "module_changed", "layer_violation"]

[layers]
# Define your architectural layers here
# Example:
# ui = ["components", "pages", "hooks"]
# services = ["api", "auth"]
# db = ["models", "repositories"]

[allowed_flows]
# Define allowed dependency flows
# Example:
# ui -> services
# services -> db
"#,
        ckb_url, webhook_url
    );

    std::fs::write(&config_path, config_content)?;

    println!("✓ Created configuration at: {}", config_path.display());
    println!();
    println!("📋 Next steps:");
    println!("  1. Add layer definitions to {}", config_path.display());
    println!("  2. Run 'cmp map' to generate initial graph");
    println!("  3. Run 'cmp health' to see architectural health");
    println!();
    println!("🔗 CKB Integration:");
    println!("  - CKB URL: {}", ckb_url);
    println!("  - Webhook URL: {}", webhook_url);
    println!();
    println!("✅ Cartographer is ready to integrate with CKB!");

    Ok(())
}

fn health_mode(root: &Path) -> Result<()> {
    use crate::api::ApiState;
    use crate::mapper::extract_skeleton;
    use crate::scanner::{is_ignored_path, scan_files_with_noise_tracking};

    println!("╔═══════════════════════════════════════════════════════════╗");
    println!("║         Cartographer - Architectural Health Report          ║");
    println!("╚═══════════════════════════════════════════════════════════╝");
    println!();

    let result = scan_files_with_noise_tracking(root)?;
    let files = result.files;
    let mapped_files: std::collections::HashMap<String, crate::mapper::MappedFile> = files
        .iter()
        .filter(|p| !is_ignored_path(p))
        .filter_map(|p| {
            let content = std::fs::read_to_string(p).ok()?;
            let mapped = extract_skeleton(p, &content);
            let rel = p
                .strip_prefix(root)
                .unwrap_or(p)
                .to_string_lossy()
                .replace('\\', "/");
            Some((rel, mapped))
        })
        .collect();

    let state = ApiState::new(root.to_path_buf());
    {
        let mut files = state.mapped_files.lock().unwrap();
        *files = mapped_files;
    }

    let graph = state.rebuild_graph().map_err(|e| anyhow::anyhow!(e))?;

    println!(
        "📊 Health Score: {:.1}/100",
        graph.metadata.health_score.unwrap_or(0.0)
    );
    println!();

    println!("📈 Statistics:");
    println!("  - Files: {}", graph.metadata.total_files);
    println!("  - Dependencies: {}", graph.metadata.total_edges);
    println!("  - Bridges: {}", graph.metadata.bridge_count.unwrap_or(0));
    println!("  - Cycles: {}", graph.metadata.cycle_count.unwrap_or(0));
    println!(
        "  - God Modules: {}",
        graph.metadata.god_module_count.unwrap_or(0)
    );
    println!(
        "  - Layer Violations: {}",
        graph.metadata.layer_violation_count.unwrap_or(0)
    );
    println!();

    if !graph.cycles.is_empty() {
        println!("🔴 Critical Issues (Cycles):");
        for (i, cycle) in graph.cycles.iter().take(3).enumerate() {
            println!(
                "  {}. {} - {}",
                i + 1,
                cycle.severity,
                cycle.nodes.join(" -> ")
            );
        }
        println!();
    }

    if graph.metadata.health_score.unwrap_or(100.0) < 70.0 {
        println!("⚠️  Architectural health is below acceptable threshold.");
        println!("   Run 'cmp map --detail extended' for more information.");
    } else {
        println!("✅ Architecture looks healthy!");
    }

    Ok(())
}

fn simulate_mode(
    root: &Path,
    module: &str,
    new_signature: Option<&str>,
    remove_signature: Option<&str>,
) -> Result<()> {
    use crate::api::ApiState;
    use crate::mapper::extract_skeleton;
    use crate::scanner::{is_ignored_path, scan_files_with_noise_tracking};

    println!("╔═══════════════════════════════════════════════════════════╗");
    println!("║         Predictive Impact Analysis                         ║");
    println!("╚═══════════════════════════════════════════════════════════╝");
    println!();

    let result = scan_files_with_noise_tracking(root)?;
    let files = result.files;
    let mapped_files: std::collections::HashMap<String, crate::mapper::MappedFile> = files
        .iter()
        .filter(|p| !is_ignored_path(p))
        .filter_map(|p| {
            let content = std::fs::read_to_string(p).ok()?;
            let mapped = extract_skeleton(p, &content);
            let rel = p
                .strip_prefix(root)
                .unwrap_or(p)
                .to_string_lossy()
                .replace('\\', "/");
            Some((rel, mapped))
        })
        .collect();

    let state = ApiState::new(root.to_path_buf());
    {
        let mut files = state.mapped_files.lock().unwrap();
        *files = mapped_files;
    }

    let change = state
        .simulate_change(module, new_signature, remove_signature)
        .map_err(|e| anyhow::anyhow!(e))?;

    println!("🎯 Target: {}", change.target_module);
    println!();
    println!("📊 Impact Analysis:");
    println!("   Risk Level: {}", change.predicted_impact.risk_level);
    println!(
        "   Health Impact: {:.1}",
        change.predicted_impact.health_impact
    );
    println!(
        "   Direct Callers: {}",
        change.predicted_impact.callers_count
    );
    println!(
        "   Direct Callees: {}",
        change.predicted_impact.callees_count
    );
    println!();

    if change.predicted_impact.will_create_cycle {
        println!("⚠️  WARNING: This change will create a circular dependency!");
    }

    if !change.predicted_impact.layer_violations.is_empty() {
        println!(
            "🚨 Layer Violations: {}",
            change.predicted_impact.layer_violations.len()
        );
        for v in &change.predicted_impact.layer_violations {
            println!(
                "   - {} -> {} ({})",
                v.source_layer,
                v.target_layer,
                v.violation_type.as_str()
            );
        }
    }

    if !change.predicted_impact.affected_modules.is_empty() {
        println!(
            "📦 Affected Modules ({}):",
            change.predicted_impact.affected_modules.len()
        );
        for m in change.predicted_impact.affected_modules.iter().take(5) {
            println!("   - {}", m);
        }
        if change.predicted_impact.affected_modules.len() > 5 {
            println!(
                "   ... and {} more",
                change.predicted_impact.affected_modules.len() - 5
            );
        }
    }

    Ok(())
}

fn evolution_mode(root: &Path, days: Option<u32>) -> Result<()> {
    use crate::api::ApiState;
    use crate::mapper::extract_skeleton;
    use crate::scanner::{is_ignored_path, scan_files_with_noise_tracking};

    println!("╔═══════════════════════════════════════════════════════════╗");
    println!("║         Architecture Evolution Report                      ║");
    println!("╚═══════════════════════════════════════════════════════════╝");
    println!();

    let result = scan_files_with_noise_tracking(root)?;
    let files = result.files;
    let mapped_files: std::collections::HashMap<String, crate::mapper::MappedFile> = files
        .iter()
        .filter(|p| !is_ignored_path(p))
        .filter_map(|p| {
            let content = std::fs::read_to_string(p).ok()?;
            let mapped = extract_skeleton(p, &content);
            let rel = p
                .strip_prefix(root)
                .unwrap_or(p)
                .to_string_lossy()
                .replace('\\', "/");
            Some((rel, mapped))
        })
        .collect();

    let state = ApiState::new(root.to_path_buf());
    {
        let mut files = state.mapped_files.lock().unwrap();
        *files = mapped_files;
    }

    let evolution = state.get_evolution(days).map_err(|e| anyhow::anyhow!(e))?;

    println!("📈 Current Status:");
    if let Some(snapshot) = evolution.snapshots.first() {
        println!("   Health Score: {:.1}/100", snapshot.health_score);
        println!("   Files: {}", snapshot.total_files);
        println!("   Dependencies: {}", snapshot.total_edges);
        println!("   Bridges: {}", snapshot.bridge_count);
        println!();
        println!("📊 Trend: {}", evolution.health_trend);
        println!();
    }

    if !evolution.debt_indicators.is_empty() {
        println!("⚠️  Debt Indicators:");
        for debt in &evolution.debt_indicators {
            println!("   • {}", debt);
        }
        println!();
    }

    println!("💡 Recommendations:");
    for rec in &evolution.recommendations {
        println!("   • {}", rec);
    }

    Ok(())
}
