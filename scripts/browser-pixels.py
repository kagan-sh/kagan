# Count pixels near the Browser check's page color. Prints one integer.
import struct
import sys
import zlib

MARKER = (255, 0, 128)
TOLERANCE = 24


def predictor(left, up, upper_left):
    estimate = left + up - upper_left
    distances = (
        abs(estimate - left),
        abs(estimate - up),
        abs(estimate - upper_left),
    )
    return (left, up, upper_left)[distances.index(min(distances))]


def rows(path):
    data = open(path, "rb").read()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise SystemExit("not a png")
    pos = 8
    width = height = color = None
    compressed = []
    while pos + 8 <= len(data):
        length, kind = struct.unpack(">I4s", data[pos : pos + 8])
        pos += 8
        chunk = data[pos : pos + length]
        pos += length + 4
        if kind == b"IHDR":
            width, height, depth, color, _, _, interlace = struct.unpack(">IIBBBBB", chunk)
            if depth != 8 or color not in (2, 6) or interlace != 0:
                raise SystemExit("unsupported png")
        elif kind == b"IDAT":
            compressed.append(chunk)
        elif kind == b"IEND":
            break
    if width is None or not compressed:
        raise SystemExit("png has no image")
    raw = zlib.decompress(b"".join(compressed))
    channels = 4 if color == 6 else 3
    stride = width * channels
    previous = bytearray(stride)
    out = []
    index = 0
    for _ in range(height):
        filter_kind = raw[index]
        index += 1
        row = bytearray(raw[index : index + stride])
        index += stride
        for x in range(stride):
            left = row[x - channels] if x >= channels else 0
            up = previous[x]
            upper_left = previous[x - channels] if x >= channels else 0
            if filter_kind == 1:
                row[x] = (row[x] + left) & 255
            elif filter_kind == 2:
                row[x] = (row[x] + up) & 255
            elif filter_kind == 3:
                row[x] = (row[x] + ((left + up) // 2)) & 255
            elif filter_kind == 4:
                row[x] = (row[x] + predictor(left, up, upper_left)) & 255
            elif filter_kind != 0:
                raise SystemExit("bad png filter")
        previous = row
        out.append((channels, row))
    return out


def near(value, target):
    return abs(value - target) <= TOLERANCE


def main():
    count = 0
    for channels, row in rows(sys.argv[1]):
        for x in range(0, len(row), channels):
            if (
                near(row[x], MARKER[0])
                and near(row[x + 1], MARKER[1])
                and near(row[x + 2], MARKER[2])
            ):
                count += 1
    print(count)


if __name__ == "__main__":
    main()
