# xmip-core-archive-sql

SQL script archive target: one item is one INSERT in a vendor-neutral script
any database loads. A technology of
[xmip-core-archive](https://github.com/IlleNilsson/xmip-core-archive).

A script line's literals — a string with its quote doubled, the bytes as
`X'…'` — are ISO SQL's, written and read by `xmip-core-library-sql`
(`sql::STANDARD`), the one SQL in the estate, so text outside ASCII
round-trips and a line cut anywhere is refused, never a panic.

This is a script on disk, not a table on a server: the archives in a SQL
server's table are the capability's `archive::sql::SqlArchive`.

## Toolchain

`rust-toolchain.toml` pins the toolchain for the whole estate. Do not change it
here.

## Verification

The included workflow is manual-only and calls the versioned shared workflow at
`IlleNilsson/.github@v1`.
