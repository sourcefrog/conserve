PYTHON=python

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
	[ ! -d test-git-staged ] || rm -r test-git-staged
	git checkout-index --prefix test-git-staged/ -a
	make -C test-git-staged check