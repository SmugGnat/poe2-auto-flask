namespace Launcher
{
    using System;
    using System.Diagnostics;
    using System.IO;
    using System.Net.Http;
    using System.Net.Http.Headers;
    using System.Threading.Tasks;
    using Newtonsoft.Json.Linq;

    /// <summary>
    ///     Updates GameHelper from the GitHub releases of the repository below. A private repository needs a
    ///     fine-grained read-only token (Contents: Read-only, only this repository) in the
    ///     <see cref="TokenFileName"/> file next to the launcher; a public one (<see cref="IsPublic"/>) works without.
    /// </summary>
    public static class AutoUpdate
    {
        public const string TokenFileName = "GITHUB_TOKEN";
        private const string Repository = "";

        /// <summary>
        ///     True in the public copy of the source (set by the export script): its releases can be read
        ///     without a token. The private repository needs the token file.
        /// </summary>
        public const bool IsPublic = false;
        private const string ReleaseAssetName = "GameHelper.zip";
        private static readonly HttpClient Client = new();
        private static string version_file_name = "VERSION.txt";
        private static string self_exe_name = AppDomain.CurrentDomain.FriendlyName;

        public static async Task<(bool, string)> IsNewVersionFoundAsync(string gameHelperDir, string token)
        {
            var versionFile = Path.Combine(gameHelperDir, version_file_name);
            if (!File.Exists(versionFile))
            {
                Console.WriteLine($"{versionFile} is missing, skipping upgrade process.");
                return (false, string.Empty);
            }

            var currentVersion = File.ReadAllText(versionFile).Trim();

            try
            {
                using var request = CreateRequest(
                    $"https://api.github.com/repos/{Repository}/releases/latest", token, "application/vnd.github+json");
                using var response = await Client.SendAsync(request);
                if (!response.IsSuccessStatusCode)
                {
                    Console.WriteLine($"GitHub answered {(int)response.StatusCode} {response.ReasonPhrase}. " +
                                      (IsPublic ? $"Is {Repository} reachable?"
                                                : $"Is the token in {TokenFileName} valid and allowed to read {Repository}?"));
                    return (false, string.Empty);
                }

                var release = JObject.Parse(await response.Content.ReadAsStringAsync());
                var latestVersion = (string)release["tag_name"];
                if (latestVersion == currentVersion)
                {
                    return (false, string.Empty);
                }

                foreach (var asset in release["assets"])
                {
                    if ((string)asset["name"] == ReleaseAssetName)
                    {
                        Console.WriteLine($"Installed version: {currentVersion}, latest release: {latestVersion}.");
                        return (true, (string)asset["url"]);
                    }
                }

                Console.WriteLine($"Release {latestVersion} has no {ReleaseAssetName}, skipping upgrade process.");
            }
            catch (Exception ex)
            {
                Console.WriteLine("Failed to determine IsNewVersionFound due to " + ex.Message);
            }

            return (false, string.Empty);
        }

        public static async Task<bool> UpgradeGameHelper(string gameHelperDir, string downloadUrl, string token)
        {
            var releaseFile = Path.Combine(gameHelperDir, "release.zip");
            Console.WriteLine($"Downloading new version from: {downloadUrl}");
            try
            {
                // GitHub redirects the asset download to a signed URL; HttpClient drops the
                // Authorization header on that redirect, which is what the signed URL expects.
                using var request = CreateRequest(downloadUrl, token, "application/octet-stream");
                using var response = await Client.SendAsync(request, HttpCompletionOption.ResponseHeadersRead);
                response.EnsureSuccessStatusCode();
                using var fs = new FileStream(releaseFile, FileMode.Create);
                await response.Content.CopyToAsync(fs);
            }
            catch (Exception ex)
            {
                Console.WriteLine("Failed to download the new version due to " + ex.Message);
                return false;
            }

            var selfExe = Path.Combine(gameHelperDir, $"{self_exe_name}.exe");
            Process.Start("powershell.exe", $"Start-sleep -Seconds 3; " +
                $"Expand-Archive -Force '{releaseFile}' '{gameHelperDir}'; " +
                $"Remove-Item -Force '{releaseFile}'; " +
                $"& '{selfExe}' -Force");
            return true;
        }

        private static HttpRequestMessage CreateRequest(string url, string token, string accept)
        {
            var request = new HttpRequestMessage(HttpMethod.Get, url);
            request.Headers.UserAgent.ParseAdd("GameHelper-Launcher");
            request.Headers.Accept.ParseAdd(accept);
            if (!string.IsNullOrEmpty(token))
            {
                request.Headers.Authorization = new AuthenticationHeaderValue("Bearer", token);
            }

            request.Headers.Add("X-GitHub-Api-Version", "2022-11-28");
            return request;
        }
    }
}
