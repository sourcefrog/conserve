# Conserve Encryption Design

**⚠️ This is not implemented yet and not even necessarily a final design.**

The overall goal of archive encryption is to protect the confidentiality and integrity of the archive from an attacker who can read or change the archive, and who does not have access to the process that makes or restores backups or to the encryption key.

The tradeoff of enabling encryption is that the user must safeguard a key. If the key is lost the backup cannot be restored; if the key is obtained by an attacker they can read the archive.

## Threat model

In all threat models we assume that attackers:

- Do not have the encryption key
- Cannot break the encryption or hashing algorithm
- Cannot observe or manipulate the Conserve processes
- Cannot exploit any implementation bugs

But we assume that attackers can:

- See the encrypted archive content
  - For example, because they control the server to which encrypted archives are written
  - Or because they can gain physical access to a drive holding backups
- See how the encrypted archive changes over time
  - For example, if they control the server hosting archives they can collect a trace of 
    file reads and writes
- Tamper with the encrypted archive content:
  - Including deleting and modifying files
  - And including modifying any unencrypted metadata files
- Cause the client to include certain known plaintexts in the backup source
  - For example, the attacked might email a file to the user, causing that file to be
    included in a backup of their home directory
  - Or, for example, the attacker might send requests to a server that are written
    into a database or log files included in the backup source
- Do any combination of these actions, over time, including
  - Injecting known plaintext and then observing what is written to the archive
  - Copying a file in the archive over another file
  - Reverting a file to its previous state

### Confidentiality goals

An attacker should not be able to:

- Read backup file content, filenames, permissions, or metadata
- Determine whether a particular string is present in any file, including
  whether the entire content of a file is equal to a known value

It is acceptable and apparently unavoidable that an attacker can observe:

- When new backups were written
- How much data changed or was added from one backup to the next
- That the archive is a Conserve archive
- Which versions of Conserve were used
- Lock/lease metadata including source hostnames and process IDs

### Integrity goals

It is unavoidable that an attacker who can modify the archive can make some
backups or parts of backups unrecoverable. (Conserve in general aims to recover
as much as possible if only some files are damaged, but broad damage by Mallory
may make the archive entirely unreadable.)

It is also unavoidable that an attacker who can record the state of the archive
at one time and rewrite it to that state later can make it as if the later
backups never happened. This may be undetectable unless the user keeps, in
some separate stable storage, a record of when backups were made.

An attacker should not be able to:

- Silently corrupt, change, or remove data, other than reverting to a previous version of the 
  archive, including:
  - Copying files to different names
  - Removing some files from the tree (other than by reverting to a moment
    when the backup was incomplete)
- Prevent a machine making a new correct backup. In other words, after 
  tampering, newly written files in a new backup will still be correct.
- Execute downgrade attacks that manipulate the backup program into 
  writing unencrypted content or using an attacker-influenced key.

For performance reasons Conserve does not throughly validate all existing
blocks when it writes a new archive, so corruption of existing
blocks may go unnoticed for some time. However `conserve validate` should detect
this corruption.

### Key management goals

The key can be stored in a file for noninteractive scheduled backups.

The key can optionally be stored in some kind of system keyring, so that it is somewhat harder to steal, e.g. so that it is only unlocked when the user is logged in. (At the price of only being available to make backups when the user is logged in, in that case.)

It's important that users keep a copy of the key in a place where it will not be lost if the backup source is lost, e.g. typically not on the same machine. The key should be concisely representable as text. These backups of the key must also be stored somewhere that the user feels is significantly less likely to be compromised than the backup storage itself, otherwise the encrytion is adding no value.

Test restores or validation should allow the user to try presenting the key as if they were doing a recovery, e.g. by typing it in or using a non-default file, even if it is normally read from a file or keyring.

It would be good to support key rotation: new keys are used to write new versions, while old data remains encrypted with an accessible through the key originally used to store it. This limits the damage if an older version of the backup key is leaked: data after it was rotated out is not readable.

If the user presents the wrong keyset this give a clear error message that it's the wrong key. (This might be in tension with allowing recovery even if some keys are missing?)

### Resilience

If one key from the keyset is lost or unavailable it should still be possible to read other backups, or even partial backups? In other words failure to decrypt one file should be loudly flagged but should not abort the whole process: the same as if one file was missing or corrupt.

### Non-goals

There is no need to support a mode where the backup program cannot read what was already written. Although there might be cases where a machine should not be able to access its own previous history, this seems somewhat niche and in tension with allowing incremental backups.

There is also no need to allow decryption without the ability to write new content.
This is probably better done by denying permission to write. Again I can conceive that in some cases the agent that restores would not need to be trusted to write, but it does seem niche.

There is no need to support rewriting an archive to use different keys. We could have eventually, instead, have a feature to copy trees from one archive to another, in unlike formats or encryption.

There is, tentatively, no need to directly support passphrases on keys. In many cases backups should be made by cron jobs and then it's not helpful to rely on the user to enter a passphrase. For desktop/laptop machines the key can be stored in the system keyring which already supports passphrase unlock.

## Approach

The format below is predicated on first migrating to storing index hunks as blocks, rather than directly in the index directory.

All blocks and hunks written by Conserve are of bounded size and will fit in memory. There is no need for streaming encryption.

This approach builds on the Tink key management abstractions.

If the archive is marked encrypted at creation, all backups into it are encrypted and encryption options must be set on all backups, restores, and other operations. (The encryption option may be set in client-local configuration, but the archive's assertions about whether encryption is expected must not be trusted, to prevent downgrade attacks.)

### Keys

An archive is encrypted and authenticated by a single Tink key set. (A Tink key set can contain multiple keys, of which one is primary and used to write new data, and all of them can be used to read existing data.)

A keyset is created with

    ; conserve create-keyset --output-file backup_home.keyset.json
    WARNING: Keep a safe copy of backup_home.keyset.json; if it is lost the archive will be unreadable.

Keysets are stored in files as json.

TODO: It would be good to also write the key to the system keyring, at least for cases where backups run while the user is logged in and the keyring is unlocked. However, it is typically very important that the user also makes a copy of the key somewhere off the source machine. Perhaps it should be written to the keyring and also to a file, so that the user can copy the file?

TODO: Will this also support storage in a cloud KMS?

The keyset files are compatible with Tinkey.

A new key can be appended to the keyset and set as primary.

    ; conserve add-key

TODO: Does Tink require separate keys for encryption and hashing, with no way to convert between them? Can we avoid exposing two keys to the user?

### Compression

When the archive is encrypted, compression of blocks (including indexes) is disabled
to prevent CRIME-like attacks.

### Block hashes

In an encrypted archive, blocks are always identified by a keyed hash using the hash key.
(In unencrypted archives blocks are identified by an unkeyed hash.)
The block hash is the hash of the uncompressed, unencrypted block content.

Specifically, the hash of a block is the Tink PRF.
(The MAC interface warns that it should be used only as an authenticator and not to generate random bytes, which seems to be what is needed here.)

This keyed hash is used in block file names and within index hunks.

When the keys are rotated, existing blocks in unchanged files can still match against their old hash. However, newly-written blocks that happen to have the same content as an existing block will get a new hash, and so will be written out as a new block.

### Block encryption

To write a block, it is first hashed, with the hash key. If the hash is already present, that's
enough, and the keyed hash can be used to refer to the block content from the index or 
meta-index. Otherwise, the block content is encrypted. (In unencrypted archives the block
would be compressed at this point; in encrypted archives it is not.)

Encryption is done using the Tink AEAD primitive, with the `AES256_GCM` key type. Tink internally generates a random IV. The encrypted file includes the Tink keyid.

The previously-computed hash is passed as the associated data.

To read a block with a given hash, the file identified by the hash is decrypted using the keyset. Tink will attempt to find any matching key using the keyid. The hash included as associated data validates that the file content corresponds to the filename.

(When reading unencrypted block files, Conserve hashes the file after it's read to check that the data is uncorrupted and matches the filename. For encrypted block files, this is unnecessary because the AEAD including the hash performs the same function.)

### Blocklist encryption

In the planned new format 7, the band directory contains one or more
"blocklists" (or maybe they should be called "meta-index" files) which contain
lists of hashes of index protos. The blocklist itself is a proto containing a
list of hashes.

Blocklists are also encrypted with AEAD. The associated data is filename of the blocklist relative to the archive root, with forward slashes.

The blocklist files are repeatedly rewritten during the backup after each index block is added, to allow recovery from an interrupted backup.

There is a limit on the number of blocks in each blocklist file (say 1000), after which the backup spills over to a new blocklist file, and the older blocklist is no longer modified.

### Backup metadata

The band head and tail files are also AEAD encrypted. (They contain non-secret metadata including start and end times and per-format metadata, but are encrypted anyhow.)

TODO: Do we want any per-format flags to be visible prior to decryption?

The band tail file includes the number of blocklist files, to detect if one of them is accidentally lost.

The band tail includes a keyed hash of the concatenation of all of the blocklist files, to detect corruption or tampering.

## Assessment

### Performance expectations

This design is expected to yield similar performance and scalability to unencrypted archives except for CPU overhead to encrypt and decrypt each block when they are written and read, respectively.

### Assessment: confidentiality

Since the hash is keyed, Eve cannot determine the correct hash for a block, and therefore cannot tell whether a block of known content is present.

Since each block is encrypted and all file content and filenames are stored in blocks, Eve cannot read file content or tree structure.

Since Tink generates a random IV for each block, IVs are never reused.

### Assessment: tampering

By the same logic as for Eve, Mallory cannot decrypt block content.

If Mallory blindly changes the content of a block file or truncates it, then 
when decrypted it will be discovered to have the wrong keyed hash, which 
will be detected as corruption.

If Mallory copies one block file in place of another the IV will be wrong, so
it will also decrypt to garbage and be detected as corruption.

Similarly, Eve cannot decrypt the blocklist files, and blind changes to them
will make them decrypt to garbage. Rearranging blocklist files will also be
caught in decryption because the IVs will be wrong.

If the band is complete (and so has a tail) then the corruption will be detectable because Eve cannot generate keyed hashes for the blocklist files.

### Assessment: downgrade attacks

It is important that the backup client must not trust the archive's assertion whether data should be encrypted or not. If a key file option is set and the archive indicates that it is not encrypted, the client should abort.

### Assessment: chosen-plaintext attacks

An attacker who can both inject chosen plaintext and observe writes to the archive 
may be able to determine whether the plaintext is already present in the archive.
For example, if the attacker injects a 1MB file (which will be written as a single
block) and observes that no new large blocks are written, then they can infer
that an identical block was already present at some point in the archive. 
(It does not necessarily prove that the content is present in the most recent tree,
only that the block was still present.)

In this draft design, this is considered acceptable because tree-wide deduplication
is desirable, and because the combination of limited control on the source
filesystem with observation of the backup directory seems somewhat unlikely.

The attacker's ability to guess at collisions has some practical bounds:
backups occur on some schedule (e.g. hourly) which rate-limits their guesses.

They can generally only observe matches at the granularity of entire blocks,
which limits their ability to do byte-at-a-time guessing. Typically blocks are
fairly large and hold multiple files, but in some cases Conserve will emit only
small data blocks, most obviously when only one small file has changed, but
also when changes have to be flushed out to finalize an index block.

The most favorable case for an attacker is if they're trying to guess whether 
a particular single-byte file is present, and they can inject new single-byte
files into an otherwise-quiescent archive. The simplest attack is to guess one
file at a time, in which case they will likely find the answer after 255 guesses.
Potentially the attacker could make multiple guesses per backup cycle, but 
they then face the risk that their small files will be combined into a single 
larger block, yielding inconclusive results. 

Interestingly, this attack can  only be done once per archive, since after each
byte is guessed it will then be present in the blockdir and future guesses will
give no signal. (At least, this is true unless/until the  versions containing
earlier guesses expire.)

This brute force guessing attack seems infeasible for bytes of nontrivial length,
especially considering that guesses are rate-limited by the backup cycle.

If, as is planned, small files are stored inline in the index then this attack
becomes infeasible for any file small enough to make guessing even remotely
feasible.

If blocks were compressed, it might be possible for an attacker to inject a 
series of chosen plaintexts and gradually measure whether they compress well 
against other files nearby in the tree. Because compression is disabled in
encrypted archives the attacker is limited to guessing at whether whole blocks
are present, which seems much less tractable.


