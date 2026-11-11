namespace GameOffsets.Objects.Components
{
    using System.Runtime.InteropServices;
    using Natives;

    [StructLayout(LayoutKind.Explicit, Pack = 1)]
    public struct RenderOffsets
    {
        [FieldOffset(0x0000)] public ComponentHeader Header;

        // Same as Positioned Component CurrentWorldPosition,
        // but this one contains Z axis; Z axis is where the HealthBar is.
        // If you want to use ground Z axis, swap current one with TerrainHeight.
        [FieldOffset(0x120)] public StdTuple3D<float> CurrentWorldPosition;

        // Changing this value will move the in-game healthbar up/down.
        // Not sure if it's really X,Y,Z or something else. They all move
        // healthbar up/down. This might be useless.
        [FieldOffset(0x12C)] public StdTuple3D<float> CharactorModelBounds;
        // [FieldOffset(0x00A0)] public StdWString ClassName;

        // Exactly the same as provided in the Positioned component.
        // [FieldOffset(0x00C0)] public float RotationCurrent;
        // [FieldOffset(0x00C4)] public float RotationFuture;
        // exe 05.09.2026: 0x13C held the same value as CurrentWorldPosition.Z in the hideout - TODO verify on uneven ground.
        [FieldOffset(0x184)] public float TerrainHeight; // exe 05.09.2026: measured 15.10.2026 on two height levels (follows the ground: z - 92.43 both times; 0x13C stayed constant and shifted the radar on stairs); = position + 0x64 as in 2024
    }
}
