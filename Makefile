CXX=clang++
CXXFLAGS=-Wall -ggdb
#-std=c++11
LIBS=-lprotobuf -lboost_filesystem -lboost_system -lglog

srcs = src/archive.cc src/band.cc src/conserve.cc src/util.cc \
       proto/conserve.pb.cc

conserve: $(srcs) 
	$(CXX) $(CXXFLAGS) -I. -o $@ $(srcs) $(LIBS)

all: protos

check: conserve cram-tests

protos: proto/conserve.pb.cc

proto/conserve.pb.cc proto/conserve.pb.h: proto/conserve.proto
	protoc --cpp_out=. proto/conserve.proto

check-staged:
	t=`mktemp -d` && \
	git checkout-index --prefix "$$t/" -a && \
	make -C "$$t" check && \
	rm -r "$$t"

cram-tests:
	PATH=`pwd`:$$PATH cram --indent=4 -v tests/*.md

clean:
	rm -f proto/conserve.pb.h proto/conserve.pb.cc
	rm -f conserve
