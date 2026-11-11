namespace GameOffsets.Natives
{
    using System;
    using System.Runtime.InteropServices;

    /// <summary>
    ///     Not sure the name of this structure. POE game uses it in a lot of different places
    ///     it might be a stdbucket, std unordered_set or std_unordered_multiset.
    ///     TODO: Create c++ HelloWorld program,
    ///     Create these structures (name it var),
    ///     Fill 1 value,
    ///     Use cheat engine on that HelloWorld Program.
    ///     HINT: use cout << &var << endl; to print memory address.
    ///     NOTE: A reader function that uses this datastructure exists
    ///     in SafeMemoryHandle class. If you modify this datastructure
    ///     modify that function too.
    /// </summary>
    ///     exe 05.09.2026: changed from 0x38 to 0x40 bytes. It is a dense hash map now: all elements
    ///     sit packed in Data, Index holds the hash slots (8 bytes each), and Capacity is the maximum
    ///     number of elements before a rehash (slots * MaxLoadFactor, e.g. 16 * 0.8 = 12).
    [StructLayout(LayoutKind.Sequential, Pack = 1)]
    public struct StdBucket
    {
        public StdVector Data; // ComponentArrayStructure
        public StdVector Index; // hash slots
        public int Capacity; // max elements before rehash
        public int PAD_0x34;
        public float MaxLoadFactor; // 0.8
        public int Unknown0;
    }
}