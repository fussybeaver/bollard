#!/bin/sh

bin/democratic-csi \
  --driver-config-file=config/local-hostpath.yaml \
  --log-level=debug \
  --server-socket=/run/docker/plugins/csi-local-path.sock \
  --csi-version=1.5.0 \
  --csi-name=csi-local-hostpath
