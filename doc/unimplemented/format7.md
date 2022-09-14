# Archive Format 7

**⚠️ Unimplemented and not yet finalized.**

This document describes a new archive format for Conserve.

## Goals

The overarching goal is to make backups faster and more compact, while also giving a foundation for archive encryption.

The format changes here, although significant, are intended to be usable within existing archives, so that users who update to a new Conserve version will see the benefits without needing to create a new archive. Old backups will remain readable with old Conserve binaries, but newer backups will only be readable by new Conserve binaries.

## Design

These changes are listed in the approximate order they will be implemented.

### Remove multi-part band ids

Band ids currently support a dashed-decimal syntax and are internally a `Vec<usize>`, but this has never been used. It will not be used in the future, and so will be removed.

### Band flags

Bands gain a new `band_flags` field stored in their `BANDHEAD` file, as a list of strings. This compliments the existing `band_version` field, which is a single string. Conserve will open a band if it understands all the named flags. This allows incremental deployment of changes without knowing in advance which version will include them.

Introduction of the `band_flags` field will increment the `band_format_version`, to ensure that old Conserve versions won't open these bands. After this point, the `band_format_version` should generally not need to change.

Many of the following changes can be indicated by flags, although they need not be _individual_ flags unless they ship individually.

### Archive flags

Similarly the archive will gain an `archive_flags` field in the `CONSERVE` file, set at archive creation time and never changed. This can be used in future to indicate that the archive is encrypted.

Importantly, all existing archives will have an absent `archive_flags`, interpreted as empty, into the future.

### Index protobufs

Indexes are serialized as protobufs rather than json. This is somewhat more compact and makes less work for compression and deserialization. Archive and band metadata remains in json to be easily readable by humans.

In particular, block hashes can then be stored as bytes rather than hex.

The filename field should probably be `bytes` rather than `string` to leave the door open to later storing non-utf8 filenames.

### Small files inline in indexes

Small files, less than about 256B, can be stored inline in the index, as protobuf bytes. Each file entry may have either a list of blocks or inline data, but not both. This avoids an extra level of indirection for small files, and may actually be smaller for small files.

### Index blocks as blobs

In 0.6, the index hunks are stored in a specific index directory. In 7, they are stored as blobs in the blob directory, addressed by their hash. This allows deduplication of index hunks, reduces the number of parallel concepts, and simplifies encryption, since only one type of bulk data object needs to be encrypted.

Index hunks will initially hold a limited number of entries, as they do in 0.6. In future there is room to improve this by splitting the index so that unchanged hunks align with previous backups, and only changed hunks are stored.

### zstd compression

A new block directory will be introduced within which blocks are zstd-compressed. The addressing is the same as at present: the BLAKE2b hash of the uncompressed content.

When reading, Conserve will try the zstd directory first and then fall back to the existing Snappy directory.

Whereas the current `d` directory has up to 4096 3-hex-digit subdirectories, the `dzstd` will have 256 2-hex-digit subdirectories, and they will all be created when the `dzstd` directory is created. This is to avoid the need to create subdirectories on the fly, which is slow. We still keep
