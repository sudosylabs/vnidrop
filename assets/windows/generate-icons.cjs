const fs = require('node:fs/promises');
const path = require('node:path');
const sharp = require('sharp');

async function main() {
    const source = await fs.readFile(path.join(__dirname, 'app-icon.svg'));
    const packageAssets = path.resolve(__dirname, '../../packaging/windows/Assets');
    const images = new Map();
    async function render(size) {
        if (!images.has(size)) {
            images.set(size, await sharp(source).resize(size, size).png().toBuffer());
        }
        return images.get(size);
    }

    await fs.writeFile(path.join(__dirname, 'app-icon.png'), await render(1024));
    const iconSizes = [16, 20, 24, 30, 32, 36, 40, 48, 60, 64, 72, 80, 96, 128, 256];
    const directory = Buffer.alloc(6 + 16 * iconSizes.length);
    directory.writeUInt16LE(1, 2);
    directory.writeUInt16LE(iconSizes.length, 4);
    const frames = [];
    let offset = directory.length;
    for (const [index, size] of iconSizes.entries()) {
        const frame = await render(size);
        const entry = 6 + index * 16;
        directory[entry] = directory[entry + 1] = size === 256 ? 0 : size;
        directory.writeUInt16LE(1, entry + 4);
        directory.writeUInt16LE(32, entry + 6);
        directory.writeUInt32LE(frame.length, entry + 8);
        directory.writeUInt32LE(offset, entry + 12);
        frames.push(frame);
        offset += frame.length;
    }
    await fs.writeFile(path.join(__dirname, 'app-icon.ico'), Buffer.concat([directory, ...frames]));

    for (const [name, size] of [['Square44x44Logo', 44], ['Square150x150Logo', 150], ['StoreLogo', 50]]) {
        for (const scale of [1, 2, 4]) {
            const suffix = scale === 1 ? '' : `.scale-${scale * 100}`;
            await fs.writeFile(path.join(packageAssets, `${name}${suffix}.png`), await render(size * scale));
        }
    }
    for (const size of [16, 20, 24, 30, 32, 36, 40, 44, 48, 60, 64, 72, 80, 96, 256]) {
        for (const theme of ['', '_altform-unplated', '_altform-lightunplated']) {
            await fs.writeFile(path.join(packageAssets, `Square44x44Logo.targetsize-${size}${theme}.png`), await render(size));
        }
    }
}

main().catch(error => { console.error(error); process.exitCode = 1; });
