namespace GameOffsets.Objects.Components
{
    using System.Runtime.InteropServices;

    [StructLayout(LayoutKind.Explicit, Pack = 1)]
    public struct PositionedOffsets
    {
        [FieldOffset(0x000)] public ComponentHeader Header;

        // Team: *(ushort *)(param_1 + 0x1e0) & 0x7fff;
        // Is Minion: *(ushort*)(param_1 + 0x1e0) >> 0xf);
        [FieldOffset(0x1E0)] public byte Reaction;
    }
}
