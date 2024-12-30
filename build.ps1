$imageName = "scanner"
$platform = "linux/arm/v7"

docker build -t $imageName --platform $platform "$PSScriptRoot/docker"

docker volume create cargo-home

docker run -it --rm `
	-v ${PSScriptRoot}:/workspace `
	-v cargo-home:/root/.cargo `
	--platform $platform `
	-w /workspace `
	$imageName `
	/bin/bash -c "cargo build --release --bin scanner_3d --target armv7-unknown-linux-gnueabihf"
