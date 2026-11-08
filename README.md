## Usage

```text
xisa-img seal <image> [-o <out>]   fill in the file size and the CRC-32C,
                                   the two fields a link cannot compute,
                                   then check the image (in place without -o)

xisa-img check <image>             print the header, or why a loader
                                   refuses the image

xisa-img --version                 this tool's version and the platform
                                   contract version it seals for
```
