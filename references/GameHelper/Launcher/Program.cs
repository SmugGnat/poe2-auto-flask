// <copyright file="Program.cs" company="None">
// Copyright (c) None. All rights reserved.
// </copyright>

namespace Launcher
{
    using System;
    using System.Diagnostics;
    using System.IO;
    using System.Threading;
    using System.Threading.Tasks;

    public static class Program
    {
        private static async Task Main(string[] args)
        {
            if (!GameHelperFinder.TryFindGameHelperExe(out var gameHelperDir, out var gameHelperLoc))
            {
                Console.WriteLine($"GameHelper.exe is also not found in {gameHelperDir}");
                Console.ReadKey();
                return;
            }

            var tokenFile = Path.Join(gameHelperDir, AutoUpdate.TokenFileName);
            var token = File.Exists(tokenFile) ? File.ReadAllText(tokenFile).Trim() : string.Empty;
            var canUpdate = AutoUpdate.IsPublic || !string.IsNullOrEmpty(token);
            if (!canUpdate)
            {
                Console.WriteLine($"No {tokenFile} found, skipping the update check. " +
                                  "Put a fine-grained GitHub token (Contents: Read-only) into that file to enable updates.");
            }

            var isWaiting = false;
            if (args.Length == 0 && canUpdate)
            {
                Console.Write("Are you waiting for a new GameHelper release? (y/n):");
                isWaiting = Console.ReadLine().ToLowerInvariant().StartsWith("y");
            }

            do
            {
                if (!canUpdate)
                {
                    break;
                }

                var (isNewVersionFound, downloadUrl) = await AutoUpdate.IsNewVersionFoundAsync(gameHelperDir, token);
                if (isNewVersionFound)
                {
                    Console.WriteLine("New gamehelper version found.");
                    if (await AutoUpdate.UpgradeGameHelper(gameHelperDir, downloadUrl, token))
                    {
                        // Returning because Launcher should auto-restart on exit.
                        return;
                    }
                    else
                    {
                        Console.WriteLine("Failed to download gamehelper new version. Please download it manually.");
                        Console.WriteLine("Press any key to continue...");
                        Console.ReadKey();
                        break;
                    }
                }
                else if (isWaiting)
                {
                    Console.WriteLine("Didn't find any new version, sleeping for 5 mins....");
                    Thread.Sleep(5 * 60 * 1000);
                }
            }
            while (isWaiting);

            try
            {
                Console.WriteLine("\n\nPreparing GameHelper...");
                var newName = MiscHelper.GenerateRandomString();
                TemporaryFileManager.Purge();
                //TODO: if functionality extends, should probably utilize an argument parser, but good for now
                if (!LocationValidator.IsGameHelperLocationGood(out var message))
                {
                    Console.WriteLine(message);
                    Console.Write("Press any key to ignore this warning.");
                    Console.ReadLine();
                }

                var gameHelperPath = GameHelperTransformer.TransformGameHelperExecutable(gameHelperDir, gameHelperLoc, newName);
                Console.WriteLine($"Starting GameHelper at '{gameHelperPath}'...");
                Process.Start(gameHelperPath);
            }
            catch (Exception ex)
            {
                Console.WriteLine($"Failed to launch GameHelper due to: {ex}");
                Console.ReadKey();
            }

            return;
        }
    }
}
