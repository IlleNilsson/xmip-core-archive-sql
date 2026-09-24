# xmip-core-archive-sql

SQL script archive target: one item is one INSERT in a vendor-neutral script
any database loads. A technology of
[xmip-core-archive](https://github.com/IlleNilsson/xmip-core-archive).

A script line is read back through `xmip-core-library-codec`'s character
reader, so text outside ASCII round-trips and a line cut anywhere is
refused, never a panic; its string literals are written and read by the
codec's `sql` module, the one SQL quoting in the estate.

This is a script on disk, not a table on a server: the archives in a SQL
server's table are the capability's `archive::sql::SqlArchive`.

## Toolchain

`rust-toolchain.toml` pins the toolchain for the whole estate. Do not change it
here.

## Verification

The included workflow is manual-only and calls the versioned shared workflow at
`IlleNilsson/.github@v1`.
