Conserve is a (design for a) robust backup program
==============================================

Copyright 2012-2013 [Martin Pool][1], mbp@sourcefrog.net.

Conserve is licensed under the [Apache License, Version 2.0][2].

**At this time Conserve is not ready for use.**

_Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License._

Conserve's homepage is: <https://github.com/sourcefrog/conserve>


Manifesto
---------

* The most important thing is that data be retrieved when it's needed;
  within reason it's worth trading off other attributes for that.

* The format should be robust: if some data is lost, it should still be
  possible to retrieve the rest of the tree.

* Use simple formats and conservative internal design, to minimize the risk of
  loss due to internal bugs.

* Well matched for high-latency, limited-bandwidth, write-once cloud
  storage.  Cloud storage typically doesn't have full filesystem semantics,
  but is very unlikely to have IO errors.  Conserve is also suitable
  for local online disk, removable storage, and remote (ssh) smart servers.

* Optional storage layers: compression (bzip2/gzip/lzo), encryption (gpg),
  redundancy (Reed-Solomon).

* Backups should always make forward progress, even if they are never
  given enough time to read the whole source or the whole destination.

* Restoring a single file or a subset of the backup must be reasonably
  fast, and must not require reading all history or the entire tree.

* Provide and encourage fast consistency checks of the backup that
  don't require reading all data back (which may be too slow to do regularly).

* Also, possibly-slow verification checks that actually do compare the backup
  to the source directory, to catch corruption or Conserve bugs.

* Send backups to multiple locations: local disk, removable disk,
  LAN servers, the cloud.

* A human oriented text UI, and a machine UI that can be used to implement
  out-of-process UIs.  Web and GUI uis.

* Set up as a cron job or daemon and then no other maintenance is needed,
  other than sometimes manually double-checking the backups can be
  restored and are complete.

* The backup archive should be a pure function of the source directory
  and history of backup operations.  (If the backup metadata includes
  a timestamp, you can pass in the timestamp to get the same result.)


Dependencies
============

Ubuntu/Debian package names:

    libprotobuf-dev
    clang
    protobuf-compiler
    make
    libgoogle-glog-dev

[1]: http://sourcefrog.net/
[2]: https://www.apache.org/licenses/LICENSE-2.0.html
