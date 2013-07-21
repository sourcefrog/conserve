AUTOMAKE_OPTIONS = foreign

bin_PROGRAMS = conserve

check: cram-tests

CXXFLAGS=-Wall -ggdb
LDADD=-lprotobuf -lboost_filesystem -lboost_system -lglog
CPPFLAGS = -I$(srcdir/src) -I.

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
	src/conserve.cc \
	src/util.cc \
	src/util.h

proto/conserve.pb.cc proto/conserve.pb.h: proto/conserve.proto
	protoc --cpp_out=. --proto_path $(srcdir) $<

check-staged:
	t=`mktemp -d` && \
	git checkout-index --prefix "$$t/" -a && \
	cd "$$t" && \
	./autogen.sh && \
	make distcheck && \
	rm -r "$$t"

cram-tests: conserve $(cram_tests)
	PATH=`pwd`:$$PATH cram --indent=4 -v $(srcdir)/tests/*md

CLEANFILES = proto/conserve.pb.h proto/conserve.pb.cc

EXTRA_DIST = \
	proto/conserve.proto \
	$(cram_tests)

cram_tests = \
	tests/backup.md \
	tests/hello.md \
	tests/help.md
