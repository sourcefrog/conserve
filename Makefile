check: cram-tests go-tests

build:
	go build ./...

go-tests:
	go test ./...

# Building the Go protos needs <http://code.google.com/p/goprotobuf/>
proto/conserve.pb.go: proto/conserve.proto
	protoc --go_out=. proto/conserve.proto

go-install:
	go install ./cli/conserve


# TODO: How to cleanly install?

check-staged:
	t=`mktemp -d` && \
	echo check-staged in "$$t" && \
	git checkout-index --prefix "$$t/" -a && \
	cd "$$t" && \
	make distcheck -j8 && \
	$(MAKE) $(MAKEFLAGS) check -j8 && \
	rm -r "$$t"

CRAM_OPTIONS = --indent=4 -v

# TODO: surely a better way to get the binary path?
gobindir = `pwd`/../../../../bin

cram-tests: go-install $(cram_tests)
	PATH=$(gobindir):$$PATH cram $(CRAM_OPTIONS) $(cram_tests)

update-cram: go-install $(cram_tests)
	PATH=$(gobindir):$$PATH cram $(CRAM_OPTIONS) -i $(cram_tests)

CLEANFILES = \
	man/conserve.1

EXTRA_DIST = \
	proto/conserve.proto \
	man/conserve.asciidoc \
	$(cram_tests)

cram_tests = \
	tests/hello.md

# TODO: reenable when the tested functionality is restored
disabled_cram_tests = \
	tests/backup.md \
	tests/help.md \
	tests/printproto.md

man/conserve.1: man/conserve.asciidoc
	[ -d man ] || mkdir man
	a2x -vv -f manpage -D man $<

show-manpage: man/conserve.1
	man $<

man1_MANS = man/conserve.1

manpages: $(man1_MANS)

