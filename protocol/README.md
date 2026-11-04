# HWLINK protocol schema

`hwlink.fbs` is the FlatBuffers schema for the HWLINK protocol
(SRS-GUI-F767 Appendix A).  It is a copy of the schema shared unchanged by
BOOT, HWVAL, the GUI and the other demo applications.

This application does not generate code from it.  The Rust crate encodes and
decodes the demo message set with the fixed-buffer FlatBuffers codec in
`../src/hwlink/` (`fb.rs`, `mod.rs`), the same code as the Rust Embassy demo:
the upstream Rust FlatBuffers runtime needs an allocator, and this
application has none.  The field numbers in `mod.rs` follow this schema;
check them whenever the schema changes.

`Message.body` is an append-only union: add new members at the end and raise
the protocol minor version (LNK-013).  The file identifier is `HWLK`.
