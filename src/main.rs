use chrono::{DateTime, Local, NaiveDateTime, Utc};
use chrono_humanize::HumanTime;
use colored::Colorize;
use git2::{Commit, Oid, Repository};
use ignore::Walk; // Add this line
use regex::Regex;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::exit;
use std::rc::Rc; // Add this import

use ptree::{print_tree, TreeBuilder};

// Configuration
const TODO_PATTERN: &str = r#"(?i)\bTODO\b(?:\((.*?)\))?(?:!|\:)?["'(]?(.*?)[)"']?$"#;

#[derive(Debug, Clone)]
struct Todo {
    file_path: PathBuf,
    line: usize,
    tags: Vec<String>,
    statement: String,
    author: String,
    commit_hash: String,
    commit_date: DateTime<Utc>,
}

fn highlight_todo(line: &str) -> String {
    let re = Regex::new(r"(?i)\bTODO\b").unwrap();
    let mut result = String::new();
    let mut last_match = 0;

    for mat in re.find_iter(line) {
        let start = mat.start();
        let end = mat.end();

        // Add the text before the match
        result.push_str(&line[last_match..start]);

        // Add the highlighted TODO
        result.push_str(&line[start..end].red().to_string());

        last_match = end;
    }

    // Add any remaining text after the last match
    result.push_str(&line[last_match..]);

    result
}

fn get_diff_with_main(repo: &Repository) -> Result<git2::Diff, git2::Error> {
    let main_branch = repo.find_branch("main", git2::BranchType::Local)?;
    let main_tree = main_branch.get().peel_to_tree()?;

    let head = repo.head()?;
    let head_tree = head.peel_to_tree()?;

    let mut opts = git2::DiffOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .show_untracked_content(true);

    repo.diff_tree_to_tree(Some(&main_tree), Some(&head_tree), Some(&mut opts))
}

fn parse_todo(line: &str) -> (Vec<String>, String) {
    let re = Regex::new(TODO_PATTERN).unwrap();
    re.captures(line).map_or_else(
        || (vec![], line.to_string()),
        |caps| {
            let tags = caps.get(1).map_or(vec![], |m| {
                m.as_str()
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .collect()
            });

            let colored_line = highlight_todo(line);

            (tags, colored_line)
        },
    )
}

fn get_repo(path: &Path) -> Result<Repository, git2::Error> {
    Repository::open(path)
}

fn get_blame_info<'a>(
    repo: &'a Repository,
    blame: &'a git2::Blame,
) -> impl Iterator<Item = (Commit<'a>, usize)> {
    blame.iter().map(move |hunk| {
        let commit = repo
            .find_commit(hunk.final_commit_id())
            .expect("Failed to find commit");
        let lines_in_hunk = hunk.lines_in_hunk();
        (commit, lines_in_hunk)
    })
}

fn is_text_file(file_path: &Path) -> bool {
    File::open(file_path).map_or(false, |mut file| {
        let mut buffer = [0; 1024];
        file.read(&mut buffer[..])
            .map_or(false, |size| !buffer[..size].contains(&0))
    })
}

fn get_todos(repo: &Repository) -> Vec<Todo> {
    let mut todos = Vec::new();
    let root_dir = repo.workdir().unwrap();

    let diff = match get_diff_with_main(repo) {
        Ok(diff) => diff,
        Err(e) => {
            eprintln!("Error getting diff with main: {e}");
            return todos;
        }
    };

    for delta in diff.deltas() {
        let diff_file = delta.new_file();

        let Some(relative_file_path) = diff_file.path() else {
            continue;
        };

        let file_path = root_dir.join(relative_file_path);
        if !file_path.is_file() || !is_text_file(&file_path) {
            continue;
        }

        let Ok(file) = File::open(&file_path) else {
            continue;
        };

        let reader = BufReader::new(file);
        let lines: Vec<_> = reader.lines().map_while(Result::ok).collect();

        // let Ok(blame) = repo.blame_file(&file_path, None) else {
        //     println!("Failed to get blame for file: {file_path:?}");
        //     continue;
        // };
        let blame = match repo.blame_file(relative_file_path, None) {
            Ok(blame) => blame,
            Err(e) => {
                println!("Failed to get blame for file: {file_path:?}: {e}");
                continue;
            }
        };

        let mut line_to_commit = HashMap::new();
        let mut current_line = 1;

        for (commit, committed_lines) in get_blame_info(repo, &blame) {
            let commit = Rc::new(commit);
            for _ in 0..committed_lines {
                line_to_commit.insert(current_line, commit.clone());
                current_line += 1;
            }
        }

        for (idx, line) in lines.iter().enumerate() {
            if line.to_lowercase().contains("todo") {
                let (tags, statement) = parse_todo(line);
                if statement.is_empty() {
                    continue;
                }

                let commit = line_to_commit.get(&(idx + 1)).cloned();
                let (author, commit_hash, commit_date) = commit.map_or_else(
                    || (String::new(), String::new(), Utc::now()),
                    |commit| {
                        (
                            commit.author().name().unwrap_or("Unknown").to_string(),
                            commit.id().to_string(),
                            DateTime::from_utc(
                                NaiveDateTime::from_timestamp(commit.time().seconds(), 0),
                                Utc,
                            ),
                        )
                    },
                );

                todos.push(Todo {
                    file_path: file_path.clone(),
                    line: idx + 1,
                    tags,
                    statement,
                    author,
                    commit_hash,
                    commit_date,
                });
            }
        }
    }

    todos
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    timestamp_nanos: i64,
    display: String,
}

fn group_todos(todos: Vec<Todo>) -> HashMap<Key, HashMap<String, HashMap<String, Vec<Todo>>>> {
    let mut grouped = HashMap::new();

    for todo in todos {
        let short_hash = &todo.commit_hash[..7];
        todo.commit_date.timestamp_nanos_opt().unwrap();
        let human_time = HumanTime::from(todo.commit_date);
        let commit_key = format!("[{short_hash}/{human_time}]");
        let author = todo.author.clone();
        let tags = if todo.tags.is_empty() {
            vec!["__no_tag__".to_string()]
        } else {
            todo.tags.clone()
        };

        let commit_key = Key {
            timestamp_nanos: todo.commit_date.timestamp_nanos_opt().unwrap(),
            display: commit_key,
        };

        for tag in tags {
            grouped
                .entry(commit_key.clone())
                .or_insert_with(HashMap::new)
                .entry(tag)
                .or_insert_with(HashMap::new)
                .entry(author.clone())
                .or_insert_with(Vec::new)
                .push(todo.clone());
        }
    }

    grouped
}

fn print_grouped_todos(
    grouped: &HashMap<Key, HashMap<String, HashMap<String, Vec<Todo>>>>,
) -> std::io::Result<()> {
    let mut sorted_commits: Vec<_> = grouped.keys().collect();
    sorted_commits.sort_by(|a, b| a.timestamp_nanos.cmp(&b.timestamp_nanos));

    for commit in sorted_commits {
        let mut tree = TreeBuilder::new(commit.display.clone());
        let tags = &grouped[commit];

        let mut sorted_tags: Vec<_> = tags.keys().collect();
        sorted_tags.sort_by_key(|&x| (x == "__no_tag__", x));

        for tag in sorted_tags {
            let authors = &tags[tag];
            let has_tag = tag != "__no_tag__";

            let parent_node = if has_tag {
                tree.begin_child(format!("🏷️ {tag}"))
            } else {
                &mut tree
            };

            let mut sorted_authors: Vec<_> = authors.keys().collect();
            sorted_authors.sort();

            for author in sorted_authors {
                let author_node = parent_node.begin_child(format!("👤 {author}"));

                for todo in &authors[author] {
                    // get path relative to CWD
                    let file_link = get_relative_or_absolute_path(&todo.file_path)?;
                    let file_link = file_link.display();

                    let file_link = format!("{}:{}", file_link, todo.line);
                    let todo_text = format!("{} - {}", file_link, todo.statement.trim());
                    author_node.add_empty_child(todo_text);
                }

                author_node.end_child();
            }

            if has_tag {
                parent_node.end_child();
            }
        }

        tree.end_child();

        let tree = tree.build();
        print_tree(&tree)?;
        println!();
    }
    Ok(())
}

fn get_relative_or_absolute_path(file_path: &Path) -> std::io::Result<PathBuf> {
    // Get the current working directory
    let current_dir = std::env::current_dir()?;

    // Attempt to get the relative path
    file_path.strip_prefix(&current_dir).map_or_else(
        |_| file_path.canonicalize(),
        |relative_path| Ok(relative_path.to_path_buf()),
    )
}

fn main() {
    let repo = match get_repo(Path::new(".")) {
        Ok(repo) => repo,
        Err(e) => {
            eprintln!("Error: {e}");
            exit(1);
        }
    };

    let todos = get_todos(&repo);

    if todos.is_empty() {
        println!("✅ No TODOs found in the repository.");
        return;
    }

    let grouped = group_todos(todos);
    print_grouped_todos(&grouped).unwrap();
}
