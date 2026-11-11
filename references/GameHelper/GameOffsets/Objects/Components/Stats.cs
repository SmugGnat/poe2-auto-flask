namespace GameOffsets.Objects.Components
{
    using System;
    using System.Runtime.InteropServices;
    using GameOffsets.Natives;

    [StructLayout(LayoutKind.Explicit, Pack = 1)]
    public struct StatsOffsets // component is 0x40 bytes long
    {
        [FieldOffset(0x00)] public ComponentHeader Header;
        [FieldOffset(0x20)] public IntPtr StatsDataPtr; // StatsStructInternal
    }

    [StructLayout(LayoutKind.Explicit, Pack = 1)]
    public struct StatsStructInternal
    {
        [FieldOffset(0xF0)] public StdVector Stats; // type StatArrayStruct
    }

    [StructLayout(LayoutKind.Sequential, Pack = 1)]
    public struct StatArrayStruct
    {
        public int key;
        public int value;
    }
}
