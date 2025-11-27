mod backend;
mod ui;

use anyhow::Result;
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
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    backend::bootstrap::set_auto_approve(cli.yes);

    // Detect the system and create appropriate backend
    let system = detect_system();
    let pm: Arc<dyn PackageManager> = create_backend(&system, cli.backend)?;

    print_info(&format!(
        "Detected: {} (using {})",
        format!("{:?}", system).cyan(),
        pm.name().green()
    ));

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
            install_packages(&pm, packages).await?;
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

        None => {
            // If packages provided directly, install them
            if !cli.packages.is_empty() {
                install_packages(&pm, cli.packages).await?;
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
            anyhow::bail!(
                "No supported package manager found for Windows. Use -b to specify a backend (winget, scoop, choco, cargo, pip, go)."
            );
        }
        System::Unknown(name) => {
            anyhow::bail!(
                "Unsupported system: {}. Use -b to specify a backend. Supported backends:\n\
                 System: apt, aur, brew, dnf, pacman, pkg, zypper\n\
                 Universal: flatpak, snap\n\
                 Language: cargo, go, pip",
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

    let result: Result<Option<Vec<Package>>> = async {
        loop {
            terminal.draw(|f| searcher.render(f))?;

            // Check if we need to search
            if searcher.needs_search() && last_search_time.elapsed() >= search_delay {
                let query = searcher.get_query().to_string();
                searcher.set_loading(true);
                searcher.mark_searched();

                // Do the search
                let results = pm.search(&query).await?;
                searcher.set_results(results);
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
