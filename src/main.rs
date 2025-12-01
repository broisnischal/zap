mod backend;
mod devtools;
mod ui;
mod update;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use colored::Colorize;
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::stdout;
use std::sync::Arc;
use std::time::Duration;

use backend::sudo;
use backend::multi::MultiBackend;
use backend::{detect_available_package_managers, detect_system, Package, PackageManager, System};
use ui::*;

#[derive(Debug, Clone, Copy, ValueEnum)]
enum BackendChoice {
    Auto,
    // System package managers
    Apt,
    Aur,
    Brew,
    Winget,
    Scoop,
    Choco,
    Dnf,
    Pacman,
    Pkg,
    Zypper,
    // Universal package managers
    Flatpak,
    Snap,
    // Language package managers
    Cargo,
    Go,
    Pip,
    Npm,
    Deno,
    Pub,
    // Special
    Dockerhub,
    Zsh,
}

#[derive(Parser)]
#[command(name = "zap")]
#[command(author = "nees")]
#[command(version = "0.1.0")]
#[command(about = "Fast cross-platform package manager", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Auto-accept all prompts
    #[arg(short = 'y', long, global = true)]
    yes: bool,

    /// Select specific package manager backend
    #[arg(short, long, value_enum, default_value = "auto", global = true)]
    backend: BackendChoice,

    /// Package names to install directly
    #[arg(trailing_var_arg = true)]
    packages: Vec<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Search for packages
    #[command(alias = "s")]
    Search {
        /// Search query
        query: String,

        /// Show detailed info for first result
        #[arg(short, long)]
        info: bool,

        /// Open interactive selector for results
        #[arg(short = 'I', long)]
        interactive: bool,
    },

    /// Install packages
    #[command(alias = "i")]
    Install {
        /// Package names to install
        #[arg(required = true)]
        packages: Vec<String>,
    },

    /// Get detailed info about a package
    Info {
        /// Package name
        package: String,
    },

    /// Interactive mode - live search and select packages
    #[command(alias = "int")]
    Interactive,

    /// Update installed packages
    Update,

    /// Show detected system info and available package managers
    System,

    /// List available package managers on this system
    #[command(alias = "pm")]
    Managers,

    /// List packages installed via current backend
    #[command(alias = "ls")]
    List,

    /// Check for zap CLI updates
    #[command(alias = "selfupdate")]
    SelfUpdate,

    /// Show curated developer tools suggestions
    DevTools,

    /// Zsh plugin management
    #[command(subcommand)]
    Zsh(ZshCommands),

    /// npm package manager commands
    #[command(subcommand)]
    Npm(NpmCommands),

    /// pip package manager commands
    #[command(subcommand)]
    Pip(PipCommands),

    /// cargo package manager commands
    #[command(subcommand)]
    Cargo(CargoCommands),

    /// go package manager commands
    #[command(subcommand)]
    Go(GoCommands),

    /// Docker Hub commands
    #[command(subcommand)]
    Docker(DockerCommands),
}

#[derive(Subcommand)]
enum NpmCommands {
    /// Install npm packages
    Install {
        /// Package names to install
        #[arg(required = true)]
        packages: Vec<String>,
    },
    /// Search npm packages
    Search {
        /// Search query
        query: String,
    },
    /// List installed npm packages
    List,
}

#[derive(Subcommand)]
enum PipCommands {
    /// Install pip packages
    Install {
        /// Package names to install
        #[arg(required = true)]
        packages: Vec<String>,
    },
    /// Search pip packages
    Search {
        /// Search query
        query: String,
    },
    /// List installed pip packages
    List,
}

#[derive(Subcommand)]
enum CargoCommands {
    /// Install cargo packages
    Install {
        /// Package names to install
        #[arg(required = true)]
        packages: Vec<String>,
    },
    /// Search cargo packages
    Search {
        /// Search query
        query: String,
    },
    /// List installed cargo packages
    List,
}

#[derive(Subcommand)]
enum ZshCommands {
    /// List available zsh plugins
    Plugins,
    /// Install zsh plugins
    Install {
        /// Plugin names to install
        #[arg(required = true)]
        plugins: Vec<String>,
    },
    /// Search zsh plugins
    Search {
        /// Search query
        query: String,
    },
}

#[derive(Subcommand)]
enum GoCommands {
    /// Install go packages
    Install {
        /// Package names to install
        #[arg(required = true)]
        packages: Vec<String>,
    },
    /// Search go packages
    Search {
        /// Search query
        query: String,
    },
    /// List installed go packages
    List,
}

#[derive(Subcommand)]
enum DockerCommands {
    /// Install (pull) Docker images
    Install {
        /// Image names to pull
        #[arg(required = true)]
        images: Vec<String>,
    },
    /// Search Docker Hub images
    Search {
        /// Search query
        query: String,
    },
    /// List pulled Docker images
    List,
    /// Run a Docker container
    Run {
        /// Image name to run
        image: String,
        /// Container name (optional)
        #[arg(short, long)]
        name: Option<String>,
        /// Additional docker run arguments
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    backend::bootstrap::set_auto_approve(cli.yes);

    // Detect the system and create appropriate backend (only if needed)
    let system = detect_system();
    
    // Some commands don't need a backend at all
    let needs_backend = !matches!(
        cli.command.as_ref(),
        Some(Commands::Zsh(ZshCommands::Plugins))
            | Some(Commands::System)
            | Some(Commands::Managers)
            | Some(Commands::SelfUpdate)
            | Some(Commands::DevTools)
    );
    
    let pm: Arc<dyn PackageManager> = if needs_backend {
        create_backend(&system, cli.backend)?
    } else {
        // Create a dummy backend for commands that don't need it
        create_backend(&system, cli.backend)?
    };
    
    // Determine if we'll need sudo for this operation
    // Install, Update, Interactive mode, and direct package installation require sudo
    let needs_sudo_for_operation = matches!(
        cli.command.as_ref(),
        Some(Commands::Install { .. })
            | Some(Commands::Update)
            | Some(Commands::Interactive)
            | Some(Commands::Search { interactive: true, .. })
    ) || !cli.packages.is_empty();
    
    // Request sudo password early if needed (before any operations that require it)
    // This ensures password is asked once at the start and reused throughout the session
    if needs_sudo_for_operation && sudo::needs_sudo() {
        sudo::ensure_password()?;
    }

    // Only print backend info if we're not using multi-backend for install and we need a backend
    let using_multi_backend = matches!(cli.backend, BackendChoice::Auto)
        && matches!(cli.command.as_ref(), Some(Commands::Install { .. }) | None);
    
    // Don't show backend message for commands that don't need it
    if needs_backend {
        if !using_multi_backend {
            print_info(&format!(
                "Detected: {} (using {})",
                format!("{:?}", system).cyan(),
                pm.name().green()
            ));
        } else {
            print_info(&format!(
                "Detected: {} (using multi-backend auto-detection)",
                format!("{:?}", system).cyan()
            ));
        }
    }

    let self_update_requested = matches!(cli.command.as_ref(), Some(Commands::SelfUpdate));
    if std::env::var("ZAP_DISABLE_UPDATE_CHECK").is_err() && !self_update_requested {
        if let Err(err) = maybe_notify_update().await {
            print_warning(&format!("Skipping update check: {}", err));
        }
    }

    match cli.command {
        Some(Commands::Search {
            query,
            info,
            interactive,
        }) => {
            let packages = search_packages(&pm, &query, info).await?;

            if interactive && !packages.is_empty() {
                let finder = FuzzyFinder::new(packages);
                let selected = finder.run()?;

                if !selected.is_empty() {
                    install_selected(&pm, selected).await?;
                }
            }
        }

        Some(Commands::Install { packages }) => {
            // Use multi-backend for auto-detection when backend is Auto
            if matches!(cli.backend, BackendChoice::Auto) {
                install_packages_multi(packages).await?;
            } else {
                install_packages(&pm, packages).await?;
            }
        }

        Some(Commands::Info { package }) => {
            show_package_info(&pm, &package).await?;
        }

        Some(Commands::Interactive) => {
            interactive_mode(&pm).await?;
        }

        Some(Commands::Update) => {
            update_packages(&pm).await?;
        }

        Some(Commands::System) => {
            show_system_info(&system, &pm);
        }

        Some(Commands::Managers) => {
            show_available_managers();
        }

        Some(Commands::List) => {
            list_installed_packages(&pm)?;
        }

        Some(Commands::SelfUpdate) => {
            run_self_update().await?;
        }

        Some(Commands::DevTools) => {
            show_devtools();
        }

        Some(Commands::Npm(cmd)) => {
            let npm_pm: Arc<dyn PackageManager> = Arc::new(backend::npm::NpmBackend::new()?);
            handle_npm_command(cmd, &npm_pm).await?;
        }

        Some(Commands::Pip(cmd)) => {
            let pip_pm: Arc<dyn PackageManager> = Arc::new(backend::pip::PipBackend::new()?);
            handle_pip_command(cmd, &pip_pm).await?;
        }

        Some(Commands::Cargo(cmd)) => {
            let cargo_pm: Arc<dyn PackageManager> = Arc::new(backend::cargo::CargoBackend::new()?);
            handle_cargo_command(cmd, &cargo_pm).await?;
        }

        Some(Commands::Go(cmd)) => {
            let go_pm: Arc<dyn PackageManager> = Arc::new(backend::go::GoBackend::new()?);
            handle_go_command(cmd, &go_pm).await?;
        }

        Some(Commands::Docker(cmd)) => {
            let docker_pm: Arc<dyn PackageManager> = Arc::new(backend::dockerhub::DockerhubBackend::new()?);
            handle_docker_command(cmd, &docker_pm).await?;
        }

        Some(Commands::Zsh(cmd)) => {
            // For Plugins command, we don't need a backend
            if matches!(cmd, ZshCommands::Plugins) {
                handle_zsh_command(cmd, &pm).await?;
            } else {
                let zsh_pm: Arc<dyn PackageManager> = Arc::new(backend::zsh::ZshBackend::new()?);
                handle_zsh_command(cmd, &zsh_pm).await?;
            }
        }

        None => {
            // If packages provided directly, install them
            if !cli.packages.is_empty() {
                // Always use multi-backend for auto-detection when packages are provided directly
                install_packages_multi(cli.packages).await?;
            } else {
                // Default to interactive mode
                interactive_mode(&pm).await?;
            }
        }
    }

    Ok(())
}

fn create_backend(system: &System, choice: BackendChoice) -> Result<Arc<dyn PackageManager>> {
    match choice {
        BackendChoice::Auto => create_auto_backend(system),
        BackendChoice::Apt => Ok(Arc::new(backend::apt::AptBackend::new()?)),
        BackendChoice::Aur => Ok(Arc::new(backend::aur::AurBackend::new()?)),
        BackendChoice::Brew => Ok(Arc::new(backend::brew::BrewBackend::new()?)),
        BackendChoice::Winget => Ok(Arc::new(backend::winget::WingetBackend::new()?)),
        BackendChoice::Scoop => Ok(Arc::new(backend::scoop::ScoopBackend::new()?)),
        BackendChoice::Choco => Ok(Arc::new(backend::choco::ChocoBackend::new()?)),
        BackendChoice::Dnf => Ok(Arc::new(backend::dnf::DnfBackend::new()?)),
        BackendChoice::Pacman => Ok(Arc::new(backend::pacman::PacmanBackend::new()?)),
        BackendChoice::Pkg => Ok(Arc::new(backend::pkg::PkgBackend::new()?)),
        BackendChoice::Zypper => Ok(Arc::new(backend::zypper::ZypperBackend::new()?)),
        BackendChoice::Flatpak => Ok(Arc::new(backend::flatpak::FlatpakBackend::new()?)),
        BackendChoice::Snap => Ok(Arc::new(backend::snap::SnapBackend::new()?)),
        BackendChoice::Cargo => Ok(Arc::new(backend::cargo::CargoBackend::new()?)),
        BackendChoice::Go => Ok(Arc::new(backend::go::GoBackend::new()?)),
        BackendChoice::Pip => Ok(Arc::new(backend::pip::PipBackend::new()?)),
        BackendChoice::Npm => Ok(Arc::new(backend::npm::NpmBackend::new()?)),
        BackendChoice::Deno => Ok(Arc::new(backend::deno::DenoBackend::new()?)),
        BackendChoice::Pub => Ok(Arc::new(backend::r#pub::PubBackend::new()?)),
        BackendChoice::Dockerhub => Ok(Arc::new(backend::dockerhub::DockerhubBackend::new()?)),
        BackendChoice::Zsh => Ok(Arc::new(backend::zsh::ZshBackend::new()?)),
    }
}

fn create_auto_backend(system: &System) -> Result<Arc<dyn PackageManager>> {
    match system {
        System::Arch => {
            let backend = backend::aur::AurBackend::new()?;
            Ok(Arc::new(backend))
        }
        System::Debian | System::Ubuntu => {
            let backend = backend::apt::AptBackend::new()?;
            Ok(Arc::new(backend))
        }
        System::Fedora => {
            let backend = backend::dnf::DnfBackend::new()?;
            Ok(Arc::new(backend))
        }
        System::OpenSUSE => {
            let backend = backend::zypper::ZypperBackend::new()?;
            Ok(Arc::new(backend))
        }
        System::FreeBSD => {
            let backend = backend::pkg::PkgBackend::new()?;
            Ok(Arc::new(backend))
        }
        System::MacOS => {
            let backend = backend::brew::BrewBackend::new()?;
            Ok(Arc::new(backend))
        }
        System::Windows => {
            if let Ok(backend) = backend::winget::WingetBackend::new() {
                return Ok(Arc::new(backend));
            }
            if let Ok(backend) = backend::scoop::ScoopBackend::new() {
                return Ok(Arc::new(backend));
            }
            if let Ok(backend) = backend::choco::ChocoBackend::new() {
                return Ok(Arc::new(backend));
            }
            // Fall back to language package managers
            if let Ok(backend) = backend::cargo::CargoBackend::new() {
                return Ok(Arc::new(backend));
            }
            if let Ok(backend) = backend::pip::PipBackend::new() {
                return Ok(Arc::new(backend));
            }
            if let Ok(backend) = backend::go::GoBackend::new() {
                return Ok(Arc::new(backend));
            }
            if let Ok(backend) = backend::npm::NpmBackend::new() {
                return Ok(Arc::new(backend));
            }
            anyhow::bail!(
                "No supported package manager found for Windows. Use -b to specify a backend (winget, scoop, choco, cargo, pip, go, npm)."
            );
        }
        System::Unknown(name) => {
            anyhow::bail!(
                "Unsupported system: {}. Use -b to specify a backend. Supported backends:\n\
                 System: apt, aur, brew, dnf, pacman, pkg, zypper\n\
                 Universal: flatpak, snap\n\
                 Language: cargo, go, pip, npm",
                name
            );
        }
    }
}

fn show_system_info(system: &System, pm: &Arc<dyn PackageManager>) {
    println!();
    println!("{}", "System Information".cyan().bold());
    println!("{}", "=".repeat(40).bright_black());
    println!("Detected OS: {:?}", system);
    println!("Package Manager: {}", pm.name());
    println!("Backend ID: {}", pm.id());
    println!();

    if let Ok(installed) = pm.list_installed() {
        println!(
            "Installed packages: {}",
            installed.len().to_string().green()
        );
    }
    println!();

    // Show available package managers
    show_available_managers();
}

fn show_available_managers() {
    let available = detect_available_package_managers();

    println!();
    println!("{}", "Available Package Managers".cyan().bold());
    println!("{}", "=".repeat(40).bright_black());

    // System package managers
    println!("\n{}", "System:".yellow());
    for pm in [
        "pacman", "aur", "apt", "dnf", "zypper", "pkg", "brew", "winget", "scoop", "choco",
    ] {
        if available.contains(&pm) {
            println!("  {} {}", "✓".green(), pm);
        }
    }

    // Universal package managers
    println!("\n{}", "Universal:".yellow());
    for pm in ["flatpak", "snap"] {
        if available.contains(&pm) {
            println!("  {} {}", "✓".green(), pm);
        }
    }

    // Language package managers
    println!("\n{}", "Language:".yellow());
    for pm in ["pip", "cargo", "go", "npm"] {
        if available.contains(&pm) {
            println!("  {} {}", "✓".green(), pm);
        }
    }

    println!();
    println!(
        "{}",
        "Use -b <backend> to select a specific package manager".bright_black()
    );
    println!();
}

fn list_installed_packages(pm: &Arc<dyn PackageManager>) -> Result<()> {
    let installed = pm.list_installed()?;

    println!();
    println!("{}", "Installed Packages".cyan().bold());
    println!("{}", "=".repeat(40).bright_black());

    if installed.is_empty() {
        println!(
            "{} {}",
            "-->".green(),
            "No packages recorded by this backend."
        );
    } else {
        for (name, version) in installed {
            println!(
                "  {} {} {}",
                "•".bright_black(),
                name.cyan().bold(),
                version.green()
            );
        }
    }
    println!();

    Ok(())
}

async fn maybe_notify_update() -> Result<()> {
    if let Some(release) = update::fetch_newer_release(env!("CARGO_PKG_VERSION")).await? {
        update::print_update_message(&release);
        println!(
            "{}",
            "Set ZAP_DISABLE_UPDATE_CHECK=1 to disable automatic checks.".bright_black()
        );
    }
    Ok(())
}

async fn run_self_update() -> Result<()> {
    match update::fetch_newer_release(env!("CARGO_PKG_VERSION")).await {
        Ok(Some(release)) => {
            update::print_update_message(&release);
        }
        Ok(None) => {
            print_success("zap is already up to date!");
        }
        Err(err) => {
            print_warning(&format!("Failed to check for zap updates: {}", err));
        }
    }
    Ok(())
}

fn show_devtools() {
    println!();
    println!("{}", "Popular Developer Tools".cyan().bold());
    println!("{}", "=".repeat(60).bright_black());
    println!();

    let tools = devtools::DevTools::all_tools();
    let mut by_category: std::collections::HashMap<&str, Vec<_>> = std::collections::HashMap::new();

    for tool in &tools {
        by_category
            .entry(tool.category)
            .or_insert_with(Vec::new)
            .push(tool);
    }

    let categories = [
        "Editor",
        "Terminal",
        "Version Control",
        "Build Tools",
        "Utilities",
        "Database",
        "Container",
        "Networking",
    ];

    for category in &categories {
        if let Some(tools) = by_category.get(category) {
            println!("{}", format!("{}:", category).yellow().bold());
            for tool in tools {
                println!(
                    "  {} {} {}",
                    "•".green(),
                    tool.name.cyan().bold(),
                    tool.description.bright_black()
                );
            }
            println!();
        }
    }

    println!();
    println!(
        "{}",
        "Tip: Type 'zap' or 'zap -b <backend>' to search interactively with autocomplete suggestions!"
            .bright_black()
    );
    println!();
}

async fn search_packages(
    pm: &Arc<dyn PackageManager>,
    query: &str,
    show_info: bool,
) -> Result<Vec<Package>> {
    print_info(&format!("Searching for '{}'...", query.cyan()));

    let packages = pm.search(query).await?;

    if packages.is_empty() {
        print_warning(&format!("No packages found for '{}'", query));
        return Ok(vec![]);
    }

    print_search_results(&packages, pm.name());

    if show_info && !packages.is_empty() {
        print_package_details(&packages[0]);
    }

    Ok(packages)
}

async fn install_packages(pm: &Arc<dyn PackageManager>, package_names: Vec<String>) -> Result<()> {
    if package_names.is_empty() {
        print_warning("No packages specified");
        return Ok(());
    }

    print_info(&format!(
        "Fetching info for {} packages...",
        package_names.len()
    ));

    // Get package info
    let refs: Vec<&str> = package_names.iter().map(|s| s.as_str()).collect();
    let packages = pm.info(&refs).await?;

    if packages.is_empty() {
        print_warning("No packages found");
        return Ok(());
    }

    // Check which packages weren't found
    let found_names: Vec<_> = packages.iter().map(|p| p.name.as_str()).collect();
    for name in &package_names {
        if !found_names.contains(&name.as_str()) {
            print_warning(&format!("Package '{}' not found", name));
        }
    }

    install_selected(pm, packages).await
}

async fn install_selected(pm: &Arc<dyn PackageManager>, packages: Vec<Package>) -> Result<()> {
    println!();
    println!("Packages to install:");
    for pkg in &packages {
        let status = if pkg.installed { " (reinstall)" } else { "" };
        println!(
            "  {} {} {}{}",
            "-->".green(),
            pkg.name.cyan().bold(),
            pkg.version.green(),
            status.bright_black()
        );
    }
    println!();

    let results = pm.install(&packages).await?;
    print_install_summary(&results);

    Ok(())
}

async fn show_package_info(pm: &Arc<dyn PackageManager>, package: &str) -> Result<()> {
    let results = pm.info(&[package]).await?;

    if let Some(pkg) = results.into_iter().next() {
        print_package_details(&pkg);
    } else {
        print_error(&format!("Package '{}' not found", package));
    }

    Ok(())
}

async fn interactive_mode(pm: &Arc<dyn PackageManager>) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut searcher = LiveSearcher::new(pm.name());
    let mut last_search_time = std::time::Instant::now();
    let search_delay = Duration::from_millis(300);

    // Show popular dev tools when starting
    let suggestions = devtools::DevTools::to_packages(devtools::DevTools::popular());
    searcher.set_suggestions(suggestions);

    let result: Result<Option<Vec<Package>>> = async {
        loop {
            terminal.draw(|f| searcher.render(f))?;

            let query = searcher.get_query().to_string();

            // Show suggestions if query is empty
            if query.is_empty() {
                if !searcher.has_suggestions() {
                    let suggestions = devtools::DevTools::to_packages(devtools::DevTools::popular());
                    searcher.set_suggestions(suggestions);
                }
            } else if query.len() == 1 {
                // For single character, show devtools suggestions
                if !searcher.has_results() {
                    let suggestions = devtools::DevTools::to_packages(devtools::DevTools::search(&query));
                    searcher.set_suggestions(suggestions);
                }
            }

            // Check if we need to search (for queries >= 2 chars)
            if searcher.needs_search() && last_search_time.elapsed() >= search_delay {
                searcher.set_loading(true);
                searcher.mark_searched();

                // Do the search - this will return top 10 results for auto-suggestions
                let results = pm.search(&query).await?;
                // Limit to top 10 for auto-suggestions
                let limited_results: Vec<Package> = results.into_iter().take(10).collect();
                searcher.set_results(limited_results);
                last_search_time = std::time::Instant::now();
            }

            if event::poll(Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    match searcher.handle_key(key) {
                        Some(SearchAction::Quit) => {
                            return Ok(None);
                        }
                        Some(SearchAction::Install(packages)) => {
                            return Ok(Some(packages));
                        }
                        None => {
                            last_search_time = std::time::Instant::now();
                        }
                    }
                }
            }
        }
    }
    .await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    // Handle result
    if let Some(packages) = result? {
        if !packages.is_empty() {
            install_selected(&pm, packages).await?;
        }
    }

    Ok(())
}

async fn update_packages(pm: &Arc<dyn PackageManager>) -> Result<()> {
    print_info("Checking for updates...");

    let updates = pm.check_updates().await?;

    if updates.is_empty() {
        print_success("All packages are up to date!");
        return Ok(());
    }

    println!();
    println!(
        "{} updates available:",
        updates.len().to_string().cyan().bold()
    );
    for pkg in &updates {
        println!(
            "  {} {} -> {}",
            pkg.name.cyan(),
            "(current)".bright_black(),
            pkg.version.green()
        );
    }
    println!();

    let results = pm.update(&updates).await?;
    print_install_summary(&results);

    Ok(())
}

/// Install packages using multi-backend auto-detection
async fn install_packages_multi(package_names: Vec<String>) -> Result<()> {
    if package_names.is_empty() {
        print_warning("No packages specified");
        return Ok(());
    }

    print_info(&format!(
        "Detecting package types and searching across all available package managers..."
    ));

    let multi = MultiBackend::new()
        .context("Failed to initialize multi-backend. Make sure at least one package manager is available.")?;
    
    let backend_count = multi.get_backends().len();
    let backend_names: Vec<String> = multi.get_backends()
        .iter()
        .map(|(id, _)| id.clone())
        .collect();
    
    print_info(&format!(
        "Searching in {} package managers: {}",
        backend_count,
        backend_names.join(", ")
    ));

    let results = multi.install_auto(package_names).await?;

    print_install_summary(&results);

    Ok(())
}

/// Handle npm subcommands
async fn handle_npm_command(cmd: NpmCommands, pm: &Arc<dyn PackageManager>) -> Result<()> {
    match cmd {
        NpmCommands::Install { packages } => {
            install_packages(pm, packages).await?;
        }
        NpmCommands::Search { query } => {
            let _packages = search_packages(pm, &query, false).await?;
        }
        NpmCommands::List => {
            list_installed_packages(pm)?;
        }
    }
    Ok(())
}

/// Handle pip subcommands
async fn handle_pip_command(cmd: PipCommands, pm: &Arc<dyn PackageManager>) -> Result<()> {
    match cmd {
        PipCommands::Install { packages } => {
            install_packages(pm, packages).await?;
        }
        PipCommands::Search { query } => {
            let _packages = search_packages(pm, &query, false).await?;
        }
        PipCommands::List => {
            list_installed_packages(pm)?;
        }
    }
    Ok(())
}

/// Handle cargo subcommands
async fn handle_cargo_command(cmd: CargoCommands, pm: &Arc<dyn PackageManager>) -> Result<()> {
    match cmd {
        CargoCommands::Install { packages } => {
            install_packages(pm, packages).await?;
        }
        CargoCommands::Search { query } => {
            let _packages = search_packages(pm, &query, false).await?;
        }
        CargoCommands::List => {
            list_installed_packages(pm)?;
        }
    }
    Ok(())
}

/// Handle go subcommands
async fn handle_go_command(cmd: GoCommands, pm: &Arc<dyn PackageManager>) -> Result<()> {
    match cmd {
        GoCommands::Install { packages } => {
            install_packages(pm, packages).await?;
        }
        GoCommands::Search { query } => {
            let _packages = search_packages(pm, &query, false).await?;
        }
        GoCommands::List => {
            list_installed_packages(pm)?;
        }
    }
    Ok(())
}

/// Handle docker subcommands
async fn handle_docker_command(cmd: DockerCommands, pm: &Arc<dyn PackageManager>) -> Result<()> {
    match cmd {
        DockerCommands::Install { images } => {
            install_packages(pm, images).await?;
        }
        DockerCommands::Search { query } => {
            let _packages = search_packages(pm, &query, false).await?;
        }
        DockerCommands::List => {
            list_installed_packages(pm)?;
        }
        DockerCommands::Run { image, name, args } => {
            // Get the dockerhub backend to access run_container method
            use backend::dockerhub::DockerhubBackend;
            if let Ok(docker_backend) = DockerhubBackend::new() {
                print_info(&format!("Running container from image: {}...", image.cyan()));
                let success = docker_backend.run_container(&image, name.as_deref(), &args).await?;
                if success {
                    print_success(&format!("Container started successfully from {}", image));
                } else {
                    print_error("Failed to start container");
                }
            } else {
                print_error("Failed to initialize Docker backend");
            }
        }
    }
    Ok(())
}

/// Handle zsh subcommands
async fn handle_zsh_command(cmd: ZshCommands, pm: &Arc<dyn PackageManager>) -> Result<()> {
    match cmd {
        ZshCommands::Plugins => {
            // List popular zsh plugins
            let popular_plugins = vec![
                "ohmyzsh/ohmyzsh",
                "zsh-users/zsh-autosuggestions",
                "zsh-users/zsh-syntax-highlighting",
                "zsh-users/zsh-completions",
                "romkatv/powerlevel10k",
                "agkozak/zsh-z",
                "Aloxaf/fzf-tab",
                "zdharma-continuum/fast-syntax-highlighting",
                "marlonrichert/zsh-autocomplete",
                "zsh-users/zsh-history-substring-search",
            ];
            
            println!();
            println!("{}", "Popular Zsh Plugins".cyan().bold());
            println!("{}", "=".repeat(50).bright_black());
            println!();
            
            // Create zsh backend to access verification methods
            let zsh_backend = backend::zsh::ZshBackend::new()?;
            
            // Show loading spinner
            use indicatif::{ProgressBar, ProgressStyle};
            let pb = ProgressBar::new(popular_plugins.len() as u64);
            pb.set_style(
                ProgressStyle::default_spinner()
                    .template("{spinner:.green} Verifying plugins... ({pos}/{len})")
                    .unwrap()
            );
            
            let mut valid_plugins = Vec::new();
            
            // Verify each plugin exists and get its info
            use std::io::Write;
            for plugin in popular_plugins {
                print!("  {} Checking {}...\r", "⏳".yellow(), plugin);
                std::io::stdout().flush().ok();
                
                if let Ok(Some(pkg)) = zsh_backend.get_repo_info(plugin).await {
                    valid_plugins.push(pkg);
                }
                
                pb.inc(1);
            }
            
            pb.finish_and_clear();
            println!(); // Clear the last status line
            
            if valid_plugins.is_empty() {
                print_warning("No valid plugins found");
                return Ok(());
            }
            
            // Sort by popularity (stars)
            valid_plugins.sort_by(|a, b| b.popularity.partial_cmp(&a.popularity).unwrap_or(std::cmp::Ordering::Equal));
            
            // Display valid plugins with installation status
            for pkg in &valid_plugins {
                let status = if pkg.installed {
                    " [installed]".green()
                } else {
                    "".normal()
                };
                
                let desc = pkg.description.as_ref()
                    .map(|d| format!(" - {}", d))
                    .unwrap_or_default();
                
                println!(
                    "  {} {}{}{}",
                    "•".green(),
                    pkg.name.cyan().bold(),
                    status,
                    desc.bright_black()
                );
            }
            
            println!();
            println!("{}", "Install with: zap zsh install <plugin-name>".bright_black());
            println!();
        }
        ZshCommands::Install { plugins } => {
            install_packages(pm, plugins).await?;
        }
        ZshCommands::Search { query } => {
            let _packages = search_packages(pm, &query, false).await?;
        }
    }
    Ok(())
}
