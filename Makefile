PYTHON=python

all: protos

lint:
	pylint --rcfile pylintrc --output-format parseable --ignore dura_pb2.py duralib

check: protos
	PYTHONPATH=.:$$PYTHONPATH $(PYTHON) -m unittest discover -v

protos: duralib/proto/dura_pb2.py

duralib/proto/__init__.py duralib/proto/dura_pb2.py: proto/dura.proto
	mkdir -p duralib/proto
	touch duralib/proto/__init__.py
	protoc --python_out=duralib/ proto/dura.proto

messages.pot:
	pygettext dura duralib/*.py

check-staged:
	t=`mktemp -d -t duralib-test` && \
	git checkout-index --prefix "$$t/" -a && \
	make -C "$$t" check && \
	rm -r "$$t"
