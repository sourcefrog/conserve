# Conserve Encryption Design

**⚠️ This is not implemented yet and not even necessarily a final design.**

The approach described below is similar to SIV ("Synthetic IV") mode of deterministic authenticated encryption, adapted to the needs of Conserve for access and reference to blocks by hashes.

A general drawback of SIV is that it is not streamable: the entire plaintext must be held in temporary storage to be first hashed and then encrypted. However, since all blocks in Conserve are of bounded size and are already held in memory for hashing and compression, this is not a problem.

## Threat model

In all threat models we assume that attackers:

- Do not have the encryption key
- Cannot break the encryption or hashing algorithm
- Cannot observe or manipulate the Conserve processes
- Cannot exploit any implementation bugs

### Confidentiality

Eve can see the encrypted archive content, but does not have
the key.

The confidentiality goal is that Eve cannot read backup tree content, filenames, permissions, or tree structure. Eve also should not be able to determine whether a block with content known to her is present in the archive. In Conserve terms this means Eve should read neither file blocks nor the index.

It is acceptable that Eve can observe:

- When new backups were written
- How much data changed or was added from one backup to the next

(These are probably unavoidable if Eve can observe writes to the archive over time.)

It is also acceptable for Eve to observe some Conserve metadata that doesn't relate to any tree content, including:

- That the archive is a Conserve archive
- Which versions of Conserve were used
- Lock/lease metadata including source hostnames and process IDs

### Tampering attacks

_Mallory_, an active attacker, can read and write the encrypted archive storage, including copying, deleting, truncating, and writing encrypted files.

It is unavoidable that Mallory can damage the archive including making some backups or parts of backups unrecoverable. (Conserve in general aims to recover as much as possible if only some files are damaged, but broad damage by Mallory may make the archive entirely unreadable.)

Mallory can also delete entire backups. Existing files are for safety reasons not modified when a new backup is added, so deleting all of the backup head/tail/index files will make it as if the backup never happened. This is not detectable, except that it may leave behind some unreferenced blocks.

However it is a goal that tampering other than deleting backup heads should be detectable and should flag an error. Mallory should never be able to change the contents of a restored tree, including substituting one file for another, omitting some entries for the tree, substituting old content into new backups, or rearranging entries within a tree.

Mallory should not be able to influence the behavior of machines writing new backups other than by making previous backups corrupt or removed entirely. In other words, after tampering, newly written files for a new backup should still be correct.

For performance reasons Conserve does not throughly validate all existing blocks when it writes a new archive, so corruption by Mallory to existing blocks may be latent for some time. However `conserve validate` should detect this corruption.

Mallory can write archive metadata files but should not be able to manipulate the backup program into writing unencrypted content. (A downgrade attack.)

### Chosen-plaintext attacks

_Chad_ can control the contents of some files and directories within the backup source. (For example they may be an otherwise-unprivileged user of a multiuser system, or they might send a file that a user downloads in to a directory that is backed up.)

Chad should not be able to control what content is restored to directories outside of those he controls, in the backups where he controls them.

### Key management goals

The key can be stored in a file for noninteractive scheduled backups.

The key can optionally be stored in some kind of system keyring, so that it is somewhat harder to steal, e.g. so that it is only unlocked when the user is logged in. (At the price of only being available to make backups when the user is logged in, in that case.)

It's important that users keep a copy of the key in a place where it will not be lost if the backup source is lost, e.g. typically not on the same machine. The key should be concisely representable as text.

Users can also choose to enter a passphrase in the terminal for manual backups or restores.

Users should have the option to choose their own passphrase so that they can memorize it, or write it on paper.

Test restores or validation should allow the user to try presenting the key as if they were doing a recovery, e.g. by typing it in or using a non-default file, even if it is normally read from a file or keyring.

### Non-goals

There is no need to support a mode where the backup program cannot read what was already written. Although there might be cases where a machine should not be able to access its own previous history, this seems somewhat niche and in tension with allowing incremental backups.

There is also no need to allow decryption without the ability to write new content.
This is probably better done by denying permission to write. Again I can conceive that in some cases the agent that restores would not need to be trusted to write, but it does seem niche.

There is no need to support rewriting an archive to use different keys. We could have eventually, instead, “copy trees from one archive to another, in unlike formats or encryption.”

## Approach

The format below is predicated on first migrating to format 7, which will store index hunks as hash-addressed blocks.

### Keys

There is a single master key for the archive, set at archive creation time and never changed. If the archive is marked encrypted at creation, all backups into it are encrypted and encryption options must be set on all backups, restores, and other operations. (The encryption option may be set in client-local configuration, but the archive's assertions about whether encryption is expected must not be trusted, to prevent downgrade attacks.)

The passphrase may be provided as a filename, or by an identifier for a system keyring key.

Some random salt for the master key is stored in the archive head metadata.

The master passphrase is an ASCII string with no trailing whitespace, from which a master key is derived.

    master_key = argon2(passphrase, salt, ???)

An archive metadata file stores a random string and the keyed hash of that random string using the master key. This is used to detect whether the correct master key has been provided for later operations.

    master_key_check = blake3("master_key_check", key=master_key)

From the master key three separate keys are derived for hashing, block encryption, and blocklist encryption. This is used as a best practice and is not believed to be strictly necessary.

    hash_key = blake3("hash_key", key=master_key)
    block_key = blake3("block_key" key=master_key)
    blocklist_key = blake3("blocklist_key", key=master_key)

### Block hashes

In an encrypted archive, blocks are always identified by a keyed hash using the derived hash key.
(In unencrypted archives blocks are identified by an unkeyed hash.)
The block hash is the hash of the uncompressed, unencrypted block content.

Specifically for encrypted archives we will use the built-in keying parameter for the BLAKE3 hash.

This keyed hash is used in block file names and within index hunks.

### Block encryption

To write a block, it is first hashed. If the hash is already present, that's enough. Otherwise, the block content is first compressed, and then encrypted using the encryption key and using the block hash as an IV.

To read a block with a given hash, the file identified by the hash is first decrypted using the encryption key and using the block hash as the IV. It is then decompressed. The decompressed content is then hashed again to check that it matches the expected content.

Specifically, the blocks are encrypted using AES-256-GCM, using the derived `block_key`, and using the first 12 bytes of the block hash as the nonce.

### Blocklist encryption

In the planned new format 7, the band directory contains one or more "blocklists" which contain lists of hashes of index protos.

The block hashes are not considered secret, because they are visible on disk. However we do want to protect against tampering with the blocklists, so that an attacker cannot add or remove blocks from the index.

Blocklists are written as an outer protobuf

    message BlocklistEnvelope {
        bytes previous_keyed_hash = 1;
        bytes blocklist_proto = 2;
        bytes keyed_hash_of_blocklist_proto = 3;
    }

    message Blocklist {
        repeated Hash index_hashes = 1;
    }

Each blocklist after the first includes a keyed hash of the previous blocklist, so that deletions or rearrangements are detectable.

The blocklist files are repeatedly rewritten during the backup after each index block is added, to allow recovery from an interrupted backup.

There is a limit on the number of blocks in each blocklist file (say 1000), after which the backup spills over to a new blocklist file, and the older blocklist is no longer modified.

### Backup metadata

The band head and tail files are not encrypted; they include the start time and other non-secret metadata.

The band tail file includes the number of blocklist files, to detect if one of them is accidentally lost.

The band tail includes a keyed hash of the concatenation of all of the blocklist files, to detect corruption or tampering.

## Assessment

### Performance expectations

This design is expected to yield similar performance and scalability to unencrypted archives except for CPU overhead to encrypt and decrypt each block when they are written and read, respectively.

### Assessment: confidentiality

Since the hash is keyed, Eve cannot determine the correct hash for a block, and therefore cannot tell whether a block of known content is present.

Since each block is encrypted and all file content and filenames are stored in blocks, Eve cannot read file content or tree structure.

Since only one block file is written for each block hash, the block hash IV is never reused.

Since there is only one blocklist file per band

Since separate encryption keys are used for blocklists and blocks, reuse of IVs between them would be harmless.

### Assessment: tampering

By the same logic as for Eve, Mallory cannot decrypt block content.

If Mallory blindly changes the content of a block file it is most likely that it will decrypt to garbage, and so decompression will fail, indicating that the file is incorrect. If Mallory truncates a file, similarly, decompression is likely to fail. If decompression of the corrupted data succeeds then the hash of the file will not be what it should be, which will be detected as corruption.

If Mallory copies one block file in place of another the IV will be wrong, so it will also decrypt to garbage and be detected as corruption.

Similarly, Eve cannot decrypt the blocklist files, and blind changes to them will make them decrypt to garbage. Rearranging blocklist files will also be caught in decryption because the IVs will be wrong.

If the band is complete (and so has a tail) then the corruption will be detectable because Eve cannot generate keyed hashes for the blocklist files.

### Assessment: downgrade attacks

It is important that the backup client must not trust the archive's assertion whether data should be encrypted or not. If a key file option is set and the archive indicates that it is not encrypted, the client should abort.

### Assessment: Chad and Eve

If Chad collaborates with Eve, they may be able to probe whether certain content is already present in the archive by looking for signs of deduplication. This is considered acceptable because tree-wide deduplication is desirable, and because the combination of limited control on the source filesystem with observation of the backup directory seems somewhat unlikely.

Chad's and Eve's ability to guess at collisions has some practical bounds: backups occur on some schedule (e.g. hourly) which rate-limits their guesses. They can generally only observe matches at the granularity of entire blocks on the order of one megabyte, which limits byte-at-a-time guessing.

Chad and Eve can also observe changing block sizes which may allow CRIME-like attacks. For example if the tree contains only two small files, and Chad controls one of them, Chad can guess at content from the other, and then Eve can observe the changing block sizes on various guesses to see if Chad has guessed correctly. (This attack is made more difficult by the fact that on later backups, the unknown file will not be rewritten into a new block unless it is changed.)

This attack is, for now, accepted as unlikely due to the combination of Chad and Eve, and the apparent need for a simple file tree to make it practical.

### Assessment: Key rotation

This design does not provide for periodic key rotation, because it is a goal of Conserve never to rewrite existing data.

If the passphrase is suspected to be compromised users should make a new archive for new backups and, potentially, delete the old archive or move it offline.
