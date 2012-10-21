PYTHON=python

lint:
	pylint -rn --output-format colorized --ignore dura_pb2.py duralib

check:
	PYTHONPATH=. $(PYTHON) -m unittest discover -v

protos:
	protoc --python_out=duralib/ proto/dura.proto

messages.pot:
	pygettext dura.py duralib/*.py
