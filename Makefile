all: conserve

check: cram-tests

CXX=clang++
CXXFLAGS=-Wall -ggdb -MD
#-std=c++11
LIBS=-lprotobuf -lboost_filesystem -lboost_system -lglog
CPPFLAGS=-Isrc -I.

srcs = src/archive.cc \
       src/backup.cc \
       src/band.cc src/conserve.cc src/util.cc \
       proto/conserve.pb.cc

objs = $(subst cc,o,$(srcs))

# TODO(mbp): More precise dependencies
$(objs): src/*.h proto/conserve.pb.h

conserve: $(objs)
	$(CXX) $(CXXFLAGS) -o $@ $(objs) $(LIBS)

protos: proto/conserve.pb.cc

proto/conserve.pb.cc proto/conserve.pb.h: proto/conserve.proto
	protoc --cpp_out=. proto/conserve.proto

check-staged:
	t=`mktemp -d` && \
	git checkout-index --prefix "$$t/" -a && \
	make -C "$$t" check && \
	rm -r "$$t"

cram-tests: conserve tests/*md
	PATH=`pwd`:$$PATH cram --indent=4 -v tests/*.md

clean:
	rm -f proto/conserve.pb.h proto/conserve.pb.cc
	rm -f conserve
	rm -f $(objs)
