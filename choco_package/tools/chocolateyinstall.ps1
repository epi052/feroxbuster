$ErrorActionPreference = 'Stop' 

$toolsDir   = "$(Split-Path -parent $MyInvocation.MyCommand.Definition)"
$version    = '2.8.0'
$url        = "https://github.com/epi052/feroxbuster/releases/download/v$version/x86-windows-feroxbuster.exe.zip"
$url64      = "https://github.com/epi052/feroxbuster/releases/download/v$version/x86_64-windows-feroxbuster.exe.zip"

$packageArgs = @{
  packageName   = $env:ChocolateyPackageName
  unzipLocation = $toolsDir
  fileType      = 'exe' #only one of these: exe, msi, msu
  url           = $url
  url64bit      = $url64
  #file         = $fileLocation

  softwareName  = 'feroxbuster*' 

  # Checksums are now required as of 0.10.0.
  # To determine checksums, you can get that from the original site if provided.
  # You can also use checksum.exe (choco install checksum) and use it
  # e.g. checksum -t sha256 -f path\to\file
  checksum      = 'e5cac59c737260233903a17706a68bac11fe0d7a15169e1c5a9637cc221e7230fd6ddbfc1a7243833dde6472ad053c033449ca8338164654f7354363da54ba88'
  checksumType  = 'sha512'
  checksum64    = 'cce58d6eacef7e12c31076f5a00fee9742a4e3fdfc69d807d98736200e50469f77359978e137ecafd87b14460845c65c6808d1f8b23ae561f7e7c637e355dee3'
  checksumType64= 'sha512'
}
Install-ChocolateyZipPackage @packageArgs # https://docs.chocolatey.org/en-us/create/functions/install-chocolateyzippackage