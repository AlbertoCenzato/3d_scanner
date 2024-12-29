$imageName = "scanner"
$platform = "linux/arm/v7"

docker build -t $imageName --platform $platform "$PSScriptRoot/docker"

docker volume create sccache

docker run -it --rm `
	-v ${PSScriptRoot}:/workspace `
	-v sccache:/root/.cache/sccache `
	--platform $platform `
	-w /workspace `
	-e SCCACHE_DIR=/root/.cache/sccache `
	$imageName `
	/bin/bash -c "cargo build --release --bin scanner_3d --target armv7-unknown-linux-gnueabihf"
