$imageName = "scanner"
$platform = "linux/arm64"
$volume = "cargo-home-arm64"

docker build -t $imageName --platform $platform "$PSScriptRoot/docker"

docker volume create $volume

docker run -it --rm `
	-v ${PSScriptRoot}:/workspace `
	-v ${volume}:/root/.cargo `
	--platform $platform `
	-w /workspace `
	$imageName `
	/bin/bash -c "cargo build --release --bin scanner_3d"
