# License text supplements

These version-specific directories contain upstream license files omitted from
some published crate archives. sources.json records the original file URLs at
the exact Git commits recorded in each crate's .cargo_vcs_info.json.

The license collector uses these files only when a crate includes no license
text. Other licenses come directly from the Cargo registry, including Slint's
LICENSES/ directories. A dependency upgrade with missing license text still
fails collection until its matching files are added.
