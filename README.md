# todo

A command-line tool to scan Git repositories for TODO comments, organizing them by commit, tag, and author.

## Output

```
📋 TODOs
├─ [commit_hash/date]
│  └─ 🏷️ Tag: tag_name
│     └─ 👤 Author: author_name
│        └─ file_path:line_number - TODO comment
```