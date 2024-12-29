$imageName = "scanner"
$platform = "linux/arm/v7"

docker build -t $imageName --platform $platform "$PSScriptRoot/docker"

docker run -it --rm `
	-v ${PSScriptRoot}:/workspace `
	-v $env:SCCACHE_DIR:/root/.cache/sccache `
	--platform $platform `
	-w /workspace `
	-e SCCACHE_DIR=/root/.cache/sccache `
	$imageName `
	/bin/bash -c "cargo build --release --bin scanner_3d"
