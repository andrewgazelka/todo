# todo

A command-line tool to scan Git repositories for TODO comments, organizing them by commit, tag, and author.

## Output

```
fmt/etc an hour ago
└─ Andrew Gazelka
   └─ Cargo.toml:305 - # todo: remove this at some point

fix up 2 hours ago
└─ Andrew Gazelka
   └─ src/daft-connect/src/convert.rs:22 - // todo: support more truncate options

update 19 hours ago
└─ Andrew Gazelka
   ├─ src/daft-connect/src/convert.rs:15 - // todo: a way to do something like tracing scopes but with errors?
   ├─ src/daft-connect/src/convert.rs:72 - // todo: test
   ├─ src/daft-connect/src/convert/expr.rs:78 - "/" => Operator::FloorDivide, // todo is this what we want?
   └─ src/daft-plan/src/builder.rs:302 - // todo: should NOT broadcast; should only set first row

stash 5 days ago
└─ Andrew Gazelka
   └─ src/daft-connect/src/lib.rs:262 - operation_id: Some(request.operation_id), // todo: impl properly

stash 6 days ago
└─ Andrew Gazelka
   └─ src/daft-connect/src/config.rs:146 - // todo: need to implement this

stash a week ago
└─ Andrew Gazelka
   ├─ src/daft-connect/proto/spark/connect/commands.proto:266 - // TODO: How do we indicate errors?
   ├─ src/daft-connect/proto/spark/connect/commands.proto:267 - // TODO: Consider adding status, last progress etc here.
   └─ src/daft-connect/proto/spark/connect/commands.proto:313 - // TODO: Consider reusing Explain from AnalyzePlanRequest message.
```
