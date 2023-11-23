.PHONY:	force

docker-image:	eosio-swagger
	docker image build -t havokrust:latest . -f docker/Dockerfile

docker-test-image:	eosio-swagger
	docker image build -t havokrust:latest . -f docker/Dockerfile --build-arg ENV=testing

docker-generate-eosio-image:	eosio-debs
	docker image build -t eosio:latest . -f docker/eosio/Dockerfile.generate --build-arg ENV=testing

docker-test-eosio-image:	eosio-debs
	docker image build -t eosio:latest . -f docker/eosio/Dockerfile --build-arg ENV=testing

EOSIO_DEBS  = docker/eosio/debs/eosio.cdt_1.6.3-1-ubuntu-18.04_amd64.deb
EOSIO_DEBS += docker/eosio/debs/eosio.cdt_1.7.0-1-ubuntu-18.04_amd64.deb
#EOSIO_DEBS += docker/eosio/debs/eosio_2.0.4-1-ubuntu-18.04_amd64.deb

eosio-debs:	${EOSIO_DEBS}

docker/eosio/debs/eosio_2.0.4-1-ubuntu-18.04_amd64.deb:
	wget https://github.com/eosio/eos/releases/download/v2.0.4/eosio_2.0.4-1-ubuntu-18.04_amd64.deb -O docker/eosio/debs/eosio_2.0.4-1-ubuntu-18.04_amd64.deb

docker/eosio/debs/eosio.cdt_1.6.3-1-ubuntu-18.04_amd64.deb:
	wget https://github.com/eosio/eosio.cdt/releases/download/v1.6.3/eosio.cdt_1.6.3-1-ubuntu-18.04_amd64.deb -O docker/eosio/debs/eosio.cdt_1.6.3-1-ubuntu-18.04_amd64.deb

docker/eosio/debs/eosio.cdt_1.7.0-1-ubuntu-18.04_amd64.deb:
	wget https://github.com/eosio/eosio.cdt/releases/download/v1.7.0/eosio.cdt_1.7.0-1-ubuntu-18.04_amd64.deb -O docker/eosio/debs/eosio.cdt_1.7.0-1-ubuntu-18.04_amd64.deb

EOSIO_SOURCE_DIR = eos
EOSIO_SWAGGER_DEST_DIR = src/swagger
EOSIO_SWAGGER_SOURCE = $(shell find ${EOSIO_SOURCE_DIR}/plugins -name "*.swagger.yaml")
EOSIO_SWAGGER_FILES = $(notdir ${EOSIO_SWAGGER_SOURCE})
EOSIO_SWAGGER_DEST = ${EOSIO_SWAGGER_FILES:%=${EOSIO_SWAGGER_DEST_DIR}/%}

${EOSIO_SWAGGER_SOURCE}:	force

eosio-swagger:	${EOSIO_SWAGGER_DEST}

${EOSIO_SWAGGER_DEST}: ${EOSIO_SWAGGER_SOURCE}
	cp ${EOSIO_SWAGGER_SOURCE} ${EOSIO_SWAGGER_DEST_DIR}/
