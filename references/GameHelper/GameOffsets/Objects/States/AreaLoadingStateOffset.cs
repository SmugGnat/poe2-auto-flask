namespace GameOffsets.Objects.States
{
    using System.Runtime.InteropServices;
    using Natives;

    [StructLayout(LayoutKind.Explicit, Pack = 1)]
    public struct AreaLoadingStateOffset
    {
        // exe 05.09.2026: all three measured live across a loading screen.
        [FieldOffset(0x3C8)] public int IsLoading;
        [FieldOffset(0x794)] public uint TotalLoadingScreenTimeMs;
        [FieldOffset(0x7E8)] public StdWString CurrentAreaName; // use isloading/totalLoadingScreenTimeMs offset diff
    }
}
