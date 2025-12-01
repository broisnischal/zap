use crate::backend::Package;

/// Curated list of popular developer tools organized by category
pub struct DevTools;

#[derive(Debug, Clone)]
pub struct ToolSuggestion {
    pub name: String,
    pub description: String,
    pub category: &'static str,
}

impl DevTools {
    /// Get all curated developer tools
    pub fn all_tools() -> Vec<ToolSuggestion> {
        let mut tools = Vec::new();

        // Text Editors & IDEs
        tools.extend(Self::editors());
        // Terminal & Shell
        tools.extend(Self::terminals());
        // Version Control
        tools.extend(Self::version_control());
        // Build Tools & Compilers
        tools.extend(Self::build_tools());
        // Development Utilities
        tools.extend(Self::utilities());
        // Database Tools
        tools.extend(Self::databases());
        // Container & Virtualization
        tools.extend(Self::containers());
        // Networking Tools
        tools.extend(Self::networking());

        tools
    }

    /// Get tools matching a query (fuzzy search)
    pub fn search(query: &str) -> Vec<ToolSuggestion> {
        let query_lower = query.to_lowercase();
        Self::all_tools()
            .into_iter()
            .filter(|tool| {
                tool.name.to_lowercase().contains(&query_lower)
                    || tool.description.to_lowercase().contains(&query_lower)
                    || tool.category.to_lowercase().contains(&query_lower)
            })
            .collect()
    }

    /// Get popular tools (top suggestions)
    pub fn popular() -> Vec<ToolSuggestion> {
        vec![
            ToolSuggestion {
                name: "tmux".to_string(),
                description: "Terminal multiplexer".to_string(),
                category: "Terminal",
            },
            ToolSuggestion {
                name: "vim".to_string(),
                description: "Vi IMproved text editor".to_string(),
                category: "Editor",
            },
            ToolSuggestion {
                name: "neovim".to_string(),
                description: "Hyperextensible Vim-based text editor".to_string(),
                category: "Editor",
            },
            ToolSuggestion {
                name: "git".to_string(),
                description: "Distributed version control system".to_string(),
                category: "Version Control",
            },
            ToolSuggestion {
                name: "curl".to_string(),
                description: "Command-line tool for transferring data".to_string(),
                category: "Networking",
            },
            ToolSuggestion {
                name: "wget".to_string(),
                description: "Non-interactive network downloader".to_string(),
                category: "Networking",
            },
            ToolSuggestion {
                name: "htop".to_string(),
                description: "Interactive process viewer".to_string(),
                category: "Utilities",
            },
            ToolSuggestion {
                name: "tree".to_string(),
                description: "Directory tree viewer".to_string(),
                category: "Utilities",
            },
            ToolSuggestion {
                name: "jq".to_string(),
                description: "Command-line JSON processor".to_string(),
                category: "Utilities",
            },
            ToolSuggestion {
                name: "fzf".to_string(),
                description: "Command-line fuzzy finder".to_string(),
                category: "Utilities",
            },
        ]
    }

    fn editors() -> Vec<ToolSuggestion> {
        vec![
            ToolSuggestion {
                name: "vim".to_string(),
                description: "Vi IMproved text editor".to_string(),
                category: "Editor",
            },
            ToolSuggestion {
                name: "neovim".to_string(),
                description: "Hyperextensible Vim-based text editor".to_string(),
                category: "Editor",
            },
            ToolSuggestion {
                name: "emacs".to_string(),
                description: "GNU Emacs extensible text editor".to_string(),
                category: "Editor",
            },
            ToolSuggestion {
                name: "nano".to_string(),
                description: "Simple text editor".to_string(),
                category: "Editor",
            },
            ToolSuggestion {
                name: "code".to_string(),
                description: "Visual Studio Code".to_string(),
                category: "Editor",
            },
            ToolSuggestion {
                name: "sublime-text".to_string(),
                description: "Sublime Text editor".to_string(),
                category: "Editor",
            },
        ]
    }

    fn terminals() -> Vec<ToolSuggestion> {
        vec![
            ToolSuggestion {
                name: "tmux".to_string(),
                description: "Terminal multiplexer".to_string(),
                category: "Terminal",
            },
            ToolSuggestion {
                name: "screen".to_string(),
                description: "Terminal multiplexer".to_string(),
                category: "Terminal",
            },
            ToolSuggestion {
                name: "zsh".to_string(),
                description: "Z shell".to_string(),
                category: "Terminal",
            },
            ToolSuggestion {
                name: "fish".to_string(),
                description: "Friendly interactive shell".to_string(),
                category: "Terminal",
            },
            ToolSuggestion {
                name: "alacritty".to_string(),
                description: "GPU-accelerated terminal emulator".to_string(),
                category: "Terminal",
            },
            ToolSuggestion {
                name: "kitty".to_string(),
                description: "GPU-based terminal emulator".to_string(),
                category: "Terminal",
            },
        ]
    }

    fn version_control() -> Vec<ToolSuggestion> {
        vec![
            ToolSuggestion {
                name: "git".to_string(),
                description: "Distributed version control system".to_string(),
                category: "Version Control",
            },
            ToolSuggestion {
                name: "git-lfs".to_string(),
                description: "Git Large File Storage".to_string(),
                category: "Version Control",
            },
            ToolSuggestion {
                name: "mercurial".to_string(),
                description: "Distributed version control system".to_string(),
                category: "Version Control",
            },
            ToolSuggestion {
                name: "subversion".to_string(),
                description: "Apache Subversion".to_string(),
                category: "Version Control",
            },
        ]
    }

    fn build_tools() -> Vec<ToolSuggestion> {
        vec![
            ToolSuggestion {
                name: "make".to_string(),
                description: "Build automation tool".to_string(),
                category: "Build Tools",
            },
            ToolSuggestion {
                name: "cmake".to_string(),
                description: "Cross-platform build system".to_string(),
                category: "Build Tools",
            },
            ToolSuggestion {
                name: "ninja".to_string(),
                description: "Small build system".to_string(),
                category: "Build Tools",
            },
            ToolSuggestion {
                name: "gcc".to_string(),
                description: "GNU Compiler Collection".to_string(),
                category: "Build Tools",
            },
            ToolSuggestion {
                name: "clang".to_string(),
                description: "C/C++ compiler".to_string(),
                category: "Build Tools",
            },
            ToolSuggestion {
                name: "rust".to_string(),
                description: "Rust compiler and toolchain".to_string(),
                category: "Build Tools",
            },
            ToolSuggestion {
                name: "go".to_string(),
                description: "Go programming language".to_string(),
                category: "Build Tools",
            },
            ToolSuggestion {
                name: "nodejs".to_string(),
                description: "Node.js JavaScript runtime".to_string(),
                category: "Build Tools",
            },
            ToolSuggestion {
                name: "python3".to_string(),
                description: "Python 3 interpreter".to_string(),
                category: "Build Tools",
            },
        ]
    }

    fn utilities() -> Vec<ToolSuggestion> {
        vec![
            ToolSuggestion {
                name: "htop".to_string(),
                description: "Interactive process viewer".to_string(),
                category: "Utilities",
            },
            ToolSuggestion {
                name: "btop".to_string(),
                description: "Modern process monitor".to_string(),
                category: "Utilities",
            },
            ToolSuggestion {
                name: "tree".to_string(),
                description: "Directory tree viewer".to_string(),
                category: "Utilities",
            },
            ToolSuggestion {
                name: "jq".to_string(),
                description: "Command-line JSON processor".to_string(),
                category: "Utilities",
            },
            ToolSuggestion {
                name: "fzf".to_string(),
                description: "Command-line fuzzy finder".to_string(),
                category: "Utilities",
            },
            ToolSuggestion {
                name: "ripgrep".to_string(),
                description: "Line-oriented search tool (rg)".to_string(),
                category: "Utilities",
            },
            ToolSuggestion {
                name: "fd".to_string(),
                description: "Simple, fast alternative to find".to_string(),
                category: "Utilities",
            },
            ToolSuggestion {
                name: "bat".to_string(),
                description: "Cat clone with syntax highlighting".to_string(),
                category: "Utilities",
            },
            ToolSuggestion {
                name: "exa".to_string(),
                description: "Modern replacement for ls".to_string(),
                category: "Utilities",
            },
            ToolSuggestion {
                name: "eza".to_string(),
                description: "Modern replacement for ls (exa fork)".to_string(),
                category: "Utilities",
            },
            ToolSuggestion {
                name: "zoxide".to_string(),
                description: "Smarter cd command".to_string(),
                category: "Utilities",
            },
            ToolSuggestion {
                name: "starship".to_string(),
                description: "Cross-shell prompt".to_string(),
                category: "Utilities",
            },
            ToolSuggestion {
                name: "direnv".to_string(),
                description: "Environment variable manager".to_string(),
                category: "Utilities",
            },
            ToolSuggestion {
                name: "tldr".to_string(),
                description: "Simplified man pages".to_string(),
                category: "Utilities",
            },
            ToolSuggestion {
                name: "cheat".to_string(),
                description: "Interactive cheat sheet".to_string(),
                category: "Utilities",
            },
        ]
    }

    fn databases() -> Vec<ToolSuggestion> {
        vec![
            ToolSuggestion {
                name: "postgresql".to_string(),
                description: "PostgreSQL database server".to_string(),
                category: "Database",
            },
            ToolSuggestion {
                name: "mysql".to_string(),
                description: "MySQL database server".to_string(),
                category: "Database",
            },
            ToolSuggestion {
                name: "sqlite".to_string(),
                description: "SQLite database".to_string(),
                category: "Database",
            },
            ToolSuggestion {
                name: "redis".to_string(),
                description: "In-memory data structure store".to_string(),
                category: "Database",
            },
            ToolSuggestion {
                name: "mongodb".to_string(),
                description: "MongoDB database".to_string(),
                category: "Database",
            },
        ]
    }

    fn containers() -> Vec<ToolSuggestion> {
        vec![
            ToolSuggestion {
                name: "docker".to_string(),
                description: "Container platform".to_string(),
                category: "Container",
            },
            ToolSuggestion {
                name: "podman".to_string(),
                description: "Container engine".to_string(),
                category: "Container",
            },
            ToolSuggestion {
                name: "docker-compose".to_string(),
                description: "Docker Compose".to_string(),
                category: "Container",
            },
            ToolSuggestion {
                name: "kubectl".to_string(),
                description: "Kubernetes command-line tool".to_string(),
                category: "Container",
            },
        ]
    }

    fn networking() -> Vec<ToolSuggestion> {
        vec![
            ToolSuggestion {
                name: "curl".to_string(),
                description: "Command-line tool for transferring data".to_string(),
                category: "Networking",
            },
            ToolSuggestion {
                name: "wget".to_string(),
                description: "Non-interactive network downloader".to_string(),
                category: "Networking",
            },
            ToolSuggestion {
                name: "httpie".to_string(),
                description: "Command-line HTTP client".to_string(),
                category: "Networking",
            },
            ToolSuggestion {
                name: "netcat".to_string(),
                description: "Network utility".to_string(),
                category: "Networking",
            },
            ToolSuggestion {
                name: "nmap".to_string(),
                description: "Network exploration tool".to_string(),
                category: "Networking",
            },
            ToolSuggestion {
                name: "tcpdump".to_string(),
                description: "Network packet analyzer".to_string(),
                category: "Networking",
            },
        ]
    }

    /// Convert suggestions to Package format for display
    pub fn to_packages(suggestions: Vec<ToolSuggestion>) -> Vec<Package> {
        suggestions
            .into_iter()
            .map(|tool| Package {
                name: tool.name,
                version: "latest".to_string(),
                description: Some(format!("[{}] {}", tool.category, tool.description)),
                popularity: 0.0,
                installed: false,
                maintainer: None,
                url: None,
                extra: crate::backend::PackageExtra::default(),
            })
            .collect()
    }
}
