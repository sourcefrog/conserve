CXX=clang++
CXXFLAGS=-Wall
#-std=c++11
LIBS=-lprotobuf -lboost_filesystem -lboost_system -lglog

srcs = conserve.cc archive.cc proto/conserve.pb.cc

conserve: $(srcs)
	$(CXX) $(CXXFLAGS) -I. -o $@ $(srcs) $(LIBS)

all: protos

check: protos
	PYTHONPATH=.:$$PYTHONPATH $(PYTHON) -m unittest discover -v

protos: proto/conserve.pb.cc

proto/conserve.pb.cc proto/conserve.pb.h: proto/conserve.proto
	protoc --cpp_out=. proto/conserve.proto

check-staged:
	t=`mktemp -d -t conservelib-test` && \
	git checkout-index --prefix "$$t/" -a && \
	make -C "$$t" check && \
	rm -r "$$t"

