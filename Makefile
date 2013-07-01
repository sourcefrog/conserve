CXX=clang++
CXXFLAGS=-Wall -ggdb
#-std=c++11
LIBS=-lprotobuf -lboost_filesystem -lboost_system -lglog

srcs = conserve.cc archive.cc proto/conserve.pb.cc band.cc \
       util.cc

conserve: $(srcs)
	$(CXX) $(CXXFLAGS) -I. -o $@ $(srcs) $(LIBS)

all: protos

check: conserve cram-tests
	./test.sh

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
