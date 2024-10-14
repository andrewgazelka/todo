use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::exit;
use std::rc::Rc;
use chrono::{DateTime, Local, NaiveDateTime, Utc};
use colored::Colorize;
use git2::{Repository, Commit, Oid};
use regex::Regex;
use ignore::Walk; // Add this line

use ptree::{print_tree, TreeBuilder};


// Configuration
const TODO_PATTERN: &str = r#"(?i)\bTODO\b(?:\((.*?)\))?(?:!|\:)?["'(]?(.*?)[)"']?$"#;
const COMMENT_PATTERN: &str = r"^\s*(#|//|/\*|\*|<!--|;)";

#[derive(Debug, Clone)]
struct Todo {
    file_path: String,
    line: usize,
    tags: Vec<String>,
    statement: String,
    author: String,
    commit_hash: String,
    commit_date: DateTime<Utc>,
}

fn is_comment(line: &str) -> bool {
    let re = Regex::new(COMMENT_PATTERN).unwrap();
    re.is_match(line)
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

fn parse_todo(line: &str) -> (Vec<String>, String) {
    let re = Regex::new(TODO_PATTERN).unwrap();
    if let Some(caps) = re.captures(line) {
        let tags = caps.get(1).map_or(vec![], |m| {
            m.as_str().split(',').map(|s| s.trim().to_string()).collect()
        });
        // todo(lol): abc
        let todo_match = caps.get(0).unwrap();

        let colored_line = highlight_todo(&line);

        (tags, colored_line)
    } else {
        (vec![], line.to_string())
    }
}

fn get_repo(path: &Path) -> Result<Repository, git2::Error> {
    Repository::open(path)
}

fn get_blame_info<'a>(repo: &'a Repository, file_path: &Path) -> Result<Vec<(Commit<'a>, usize)>, git2::Error> {
    let blame = repo.blame_file(file_path, None)?;
    let mut result = Vec::new();

    for hunk in blame.iter() {
        let commit = repo.find_commit(hunk.final_commit_id())?;
        let lines_in_hunk = hunk.lines_in_hunk();
        result.push((commit, lines_in_hunk));
    }

    Ok(result)
}

fn is_text_file(file_path: &Path) -> bool {
    if let Ok(mut file) = File::open(file_path) {
        let mut buffer = [0; 1024];
        if let Ok(size) = file.read(&mut buffer[..]) {
            !buffer[..size].contains(&0)
        } else {
            false
        }
    } else {
        false
    }
}

fn get_todos(repo: &Repository) -> Vec<Todo> {
    let mut todos = Vec::new();
    let root_dir = repo.workdir().unwrap();

    // Use the ignore::Walk instead of walkdir::WalkDir
    for entry in Walk::new(root_dir) {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };

        let file_path = entry.path();
        if !file_path.is_file() || !is_text_file(file_path) {
            continue;
        }

        let relative_path = file_path.strip_prefix(root_dir).unwrap();

        if let Ok(file) = File::open(file_path) {
            let reader = BufReader::new(file);
            let mut has_todo = false;
            let lines: Vec<_> = reader.lines().filter_map(|l| l.ok()).collect();

            for (idx, line) in lines.iter().enumerate() {
                if line.to_lowercase().contains("todo") {
                    has_todo = true;
                    break;
                }
            }

            if !has_todo {
                continue;
            }

            let blame_info = get_blame_info(repo, relative_path).unwrap_or_default();
            let mut line_to_commit = HashMap::new();
            let mut current_line = 1;

            for (commit, committed_lines) in blame_info {
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
                    let (author, commit_hash, commit_date) = if let Some(commit) = commit {
                        (
                            commit.author().name().unwrap_or("Unknown").to_string(),
                            commit.id().to_string(),
                            DateTime::from_utc(
                                NaiveDateTime::from_timestamp(commit.time().seconds(), 0),
                                Utc,
                            ),
                        )
                    } else {
                        (
                            "Uncommitted".to_string(),
                            "Uncommitted".to_string(),
                            Utc::now(),
                        )
                    };

                    todos.push(Todo {
                        file_path: relative_path.to_string_lossy().to_string(),
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
    }

    todos
}

fn group_todos(todos: Vec<Todo>) -> HashMap<String, HashMap<String, HashMap<String, Vec<Todo>>>> {
    let mut grouped = HashMap::new();

    for todo in todos {
        let commit_key = format!("[{}/{}]", todo.commit_hash, todo.commit_date.format("%Y-%m-%d"));
        let author = todo.author.clone();
        let tags = if todo.tags.is_empty() {
            vec!["__no_tag__".to_string()]
        } else {
            todo.tags.clone()
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

fn print_grouped_todos(grouped: HashMap<String, HashMap<String, HashMap<String, Vec<Todo>>>>) -> std::io::Result<()> {
    let mut tree = TreeBuilder::new("📋 TODOs".to_string());

    let mut sorted_commits: Vec<_> = grouped.keys().collect();
    sorted_commits.sort_by(|a, b| {
        let date_a = a.split('/').nth(1).unwrap().trim_end_matches(']');
        let date_b = b.split('/').nth(1).unwrap().trim_end_matches(']');
        date_b.cmp(date_a)
    });

    for commit in sorted_commits {
        let commit_node = tree.begin_child(commit.to_string());
        let tags = &grouped[commit];

        let mut sorted_tags: Vec<_> = tags.keys().collect();
        sorted_tags.sort_by_key(|&x| (x == "__no_tag__", x));

        for tag in sorted_tags {
            let authors = &tags[tag];
            let tag_display = if tag == "__no_tag__" { "No Tag" } else { tag };
            let tag_node = commit_node.begin_child(format!("🏷️ Tag: {}", tag_display));

            let mut sorted_authors: Vec<_> = authors.keys().collect();
            sorted_authors.sort();

            for author in sorted_authors {
                let author_node = tag_node.begin_child(format!("👤 Author: {}", author));

                for todo in &authors[author] {
                    let file_link = format!("{}:{}", todo.file_path, todo.line);
                    let todo_text = format!(
                        "{} - {}",
                        file_link,
                        todo.statement.trim()
                    );
                    author_node.add_empty_child(todo_text);
                }

                author_node.end_child();
            }

            tag_node.end_child();
        }

        commit_node.end_child();
    }

    let tree = tree.build();

    print_tree(&tree)
}

fn main() {
    let repo = match get_repo(Path::new(".")) {
        Ok(repo) => repo,
        Err(e) => {
            eprintln!("Error: {}", e);
            exit(1);
        }
    };

    let todos = get_todos(&repo);

    if todos.is_empty() {
        println!("✅ No TODOs found in the repository.");
        return;
    }

    let grouped = group_todos(todos);
    print_grouped_todos(grouped).unwrap();
}