AUTOMAKE_OPTIONS = foreign subdir-objects

bin_PROGRAMS = conserve

check: cram-tests

CXXFLAGS += -Wall
LDADD = -lprotobuf -lboost_filesystem -lboost_system -lglog -lbz2

nodist_conserve_SOURCES = proto/conserve.pb.cc

BUILT_SOURCES = proto/conserve.pb.h proto/conserve.pb.cc

conserve_SOURCES = \
	src/archive.cc \
	src/archive.h \
	src/backup.cc \
	src/backup.h \
	src/band.cc \
	src/band.h \
	src/block.cc \
	src/block.h \
	src/blockreader.cc \
	src/blockreader.h \
	src/blockwriter.cc \
	src/blockwriter.h \
	src/bzdatawriter.cc \
	src/bzdatawriter.h \
	src/conserve.cc \
	src/datareader.cc \
	src/datareader.h \
	src/exitcode.h \
	src/filecopy.cc \
	src/filecopy.h \
	src/printproto.cc \
	src/printproto.h \
	src/problem.cc \
	src/problem.h \
	src/restore.cc \
	src/restore.h \
	src/util.cc \
	src/util.h \
	src/validate.cc \
	src/validate.h

# Building the Go protos needs <http://code.google.com/p/goprotobuf/>
proto/conserve.pb.go: proto/conserve.proto
	protoc --go_out=. proto/conserve.proto

proto/conserve.pb.cc proto/conserve.pb.h: proto/conserve.proto
	protoc --cpp_out=. --proto_path $(srcdir) $<

check-staged:
	t=`mktemp -d` && \
	echo check-staged in "$$t" && \
	cd $(srcdir) && git checkout-index --prefix "$$t/" -a && \
	cd "$$t" && \
	./autogen.sh && \
	mkdir _build && cd _build && \
	../configure --prefix "$$t/_installprefix" && \
	$(MAKE) $(MAKEFLAGS) distcheck -j8 && \
	$(MAKE) $(MAKEFLAGS) install -j8 && \
	rm -r "$$t"

CRAM_OPTIONS = --indent=4 -v

cram-tests: conserve $(cram_tests)
	PATH=`pwd`:$$PATH cram $(CRAM_OPTIONS) $(srcdir)/tests/*md

update-cram: conserve $(cram_tests)
	PATH=`pwd`:$$PATH cram $(CRAM_OPTIONS) -i $(srcdir)/tests/*md

CLEANFILES = \
	proto/conserve.pb.h proto/conserve.pb.cc \
	man/conserve.1

EXTRA_DIST = \
	proto/conserve.proto \
	man/conserve.asciidoc \
	$(cram_tests)

cram_tests = \
	tests/backup.md \
	tests/hello.md \
	tests/help.md \
	tests/printproto.md


man/conserve.1: man/conserve.asciidoc
	[ -d man ] || mkdir man
	a2x -vv -f manpage -D man $<

show-manpage: man/conserve.1
	man $<

man1_MANS = man/conserve.1
