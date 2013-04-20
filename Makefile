CXX=clang++
CXXFLAGS=-Wall 
LIBS=-lprotobuf

dura: dura.cc protos
	$(CXX) $(CXXFLAGS) -I. $(LIBS) -o $@ dura.cc proto/dura.pb.cc

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

