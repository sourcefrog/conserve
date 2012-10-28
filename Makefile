PYTHON=python

all: protos

lint:
	pylint --rcfile pylintrc --output-format parseable --ignore dura_pb2.py duralib

check: protos
	PYTHONPATH=. $(PYTHON) -m unittest discover -v

protos:
	mkdir -p duralib/proto
	touch duralib/proto/__init__.py
	protoc --python_out=duralib/ proto/dura.proto

messages.pot:
	pygettext dura.py duralib/*.py

check-staged:
	t=`mktemp -d --suffix .duralib-test` && \
	git checkout-index --prefix "$$t/" -a && \
	make -C "$$t" check && \
	rm -r "$$t"