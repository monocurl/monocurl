using Bridge;
namespace Monocurl
{
    internal static class Monocurl
    {
        public static readonly string NODE_FILE_TYPE = "mcf";
        public static readonly string NODE_FILE_FILTER = "Monocurl File |*.mcf";

        [STAThread]
        static void Main()
        {
            ApplicationConfiguration.Initialize();

            Bridge.Bridge.init();

            Application.ApplicationExit += OnApplicationExit;
            Application.Run(new Landing());
        }

        static void OnApplicationExit(object sender, EventArgs e)
        {
            Bridge.Bridge.free();
        }
    }
}