docker run --gpus all --pid=host --net=host \
--name tensat \
-it \
--mount type=bind,source="/home/jh2422/thesis/tensat",target=/usr/tensat \
--mount type=bind,source="/home/jh2422/thesis/egg",target=/usr/egg \
--mount type=bind,source="/home/jh2422/thesis/taso",target=/usr/TASO \
--mount type=bind,source="/home/jh2422/thesis/rmcts",target=/usr/rmcts \
--mount type=bind,source="/home/jh2422/thesis/experiments",target=/usr/experiments \
tensat:1.0 bash
