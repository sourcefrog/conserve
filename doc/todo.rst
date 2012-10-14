To be able to make backups at all:

* Start unit tests 
* Break output into blocks, just on rough file size
* Proper command line parsing
* Recurse through directory?
* Exclusions
* Auto-exclusions of cache directories
* Call out to non-local filesystems
* Compress output
* Extract files back out
* Internal validation of indexes (all sizes line up)
* Validate index against contents of data files

* Check against file hashes returned by the storage system:
  - Google Cloud Storage returns the md5 as the etag
  - Amazon returns md5 as Content-MD5
  - maybe change to just md5 for our own hashes
