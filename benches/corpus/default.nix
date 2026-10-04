{
  lib,
  runCommand,
  fetchurl,
  gzip,
}:
let
  sources = [
    {
      name = "alice-pg11.txt";
      src = fetchurl {
        url = "https://www.gutenberg.org/cache/epub/11/pg11.txt";
        sha256 = "01b38ea4c710a84bc18d0bd41271a5a1a92b94e97b2812f4dece97d4a694725e";
      };
      # Starts after the "*** START OF" Gutenberg header line.
      skip = 909;
      count = 24559;
    }
    {
      name = "sqlite-btree.c";
      src = fetchurl {
        url = "https://raw.githubusercontent.com/sqlite/sqlite/version-3.53.4/src/btree.c";
        sha256 = "c0982890bcd01b479c66066030e32d15b88a2dce2dd9fd67fa301211555f7a94";
      };
      count = 24548;
    }
    {
      name = "wikidata-Q100020.json";
      src = fetchurl {
        name = "wikidata-Q100020-r2525618092.json";
        url = "https://www.wikidata.org/wiki/Special:EntityData/Q100020.json?revision=2525618092";
        sha256 = "ae6d5c21e2b919532765ae866e619ef4d37a6a53363397f4ed21ddb620fd2a0a";
      };
      count = 15887; # whole document
    }
    {
      name = "nasa-http-jul95.log";
      src = fetchurl {
        url = "https://ita.ee.lbl.gov/traces/NASA_access_log_Jul95.gz";
        sha256 = "199109ed0f273e095da6ccd5fc9dc4cd8bb58daa06d62135e62090fea9d27488";
      };
      gunzip = true;
      count = 16300;
    }
  ];

  cut =
    {
      name,
      src,
      skip ? 0,
      count,
      gunzip ? false,
    }:
    let
      input = if gunzip then "<(gzip -dc ${src} 2>/dev/null)" else "${src}";
    in
    "dd if=${input} of=$out/${name} iflag=skip_bytes,count_bytes,fullblock"
    + " skip=${toString skip} count=${toString count} status=none";
in
runCommand "bench-corpus" { nativeBuildInputs = [ gzip ]; } ''
  mkdir $out
  ${lib.concatMapStringsSep "\n" cut sources}
''
