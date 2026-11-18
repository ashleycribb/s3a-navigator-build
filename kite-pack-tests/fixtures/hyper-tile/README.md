Hyper-tile and inconsistent-header fixtures generated with
s3a-engine at pin 2cf2b45fb1a3375735687252d93a612fb6c9f773.

- at08_sealed_manifest.s3a — sealed, type-82-free, tile 78 with
  "THIS IS AN S3A FILE" and seal title/creator (AT-08, AT-10)
- no_tile_78.s3a — unsealed hyper-tile with no tile 78 (AT-09)
- unsealed_with_manifest.s3a — unsealed hyper-tile with a valid tile 78 (AT-72)
- two_manifests.s3a — sealed file with two committed valid tile-78 tiles (AT-65)
- manifest_not_utf8.s3a — sealed file whose tile 78 is not valid UTF-8 (AT-64)
- edge_flag_128kib.s3a — FLAG_PROFILE_EDGE with 128 KiB stride (AT-66)
- size_4kb_stride_8192.s3a — FLAG_TILE_SIZE_4KB with stride 8192 (AT-66)
- both_size_flags.s3a — both 4 KiB and 8 KiB size flags (AT-66)
- stride_65536.s3a — tile_stride 65536 (AT-97)
