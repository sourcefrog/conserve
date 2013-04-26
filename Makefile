CXX=clang++
CXXFLAGS=-Wall -std=c++11
LIBS=-lprotobuf -lboost_program_options -lboost_filesystem \
     -lboost_system -lglog

srcs = dura.cc archive.cc

dura: $(srcs)
	$(CXX) $(CXXFLAGS) -I. $(LIBS) -o $@ $(srcs) proto/dura.pb.cc

all: protos

check: protos
	PYTHONPATH=.:$$PYTHONPATH $(PYTHON) -m unittest discover -v

protos: proto/dura.pb.cc

proto/dura.pb.cc proto/dura.pb.h: proto/dura.proto
	protoc --cc_out=. proto/dura.proto

check-staged:
	t=`mktemp -d -t duralib-test` && \
	git checkout-index --prefix "$$t/" -a && \
	make -C "$$t" check && \
	rm -r "$$t"

