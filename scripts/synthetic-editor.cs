// Short-lived synthetic fixture. Never reads the user's clipboard or documents.
using System;
using System.Drawing;
using System.Runtime.InteropServices;
using System.Threading;
using System.Windows.Forms;

class LossySyntheticEditor {
    [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll")] static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] static extern bool AttachThreadInput(uint a, uint b, bool attach);
    [DllImport("kernel32.dll")] static extern uint GetCurrentThreadId();
    [DllImport("user32.dll")] static extern bool OpenClipboard(IntPtr hwnd);
    [DllImport("user32.dll")] static extern bool EmptyClipboard();
    [DllImport("user32.dll")] static extern bool CloseClipboard();
    [DllImport("user32.dll")] static extern IntPtr SetClipboardData(uint format, IntPtr memory);
    [DllImport("kernel32.dll")] static extern IntPtr GlobalAlloc(uint flags, UIntPtr bytes);
    [DllImport("kernel32.dll")] static extern IntPtr GlobalLock(IntPtr memory);
    [DllImport("kernel32.dll")] static extern bool GlobalUnlock(IntPtr memory);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern uint RegisterClipboardFormat(string format);
    static void CopyData(IntPtr owner, uint format, byte[] bytes) {
        var memory = GlobalAlloc(2, (UIntPtr)bytes.Length);
        var pointer = GlobalLock(memory);
        Marshal.Copy(bytes, 0, pointer, bytes.Length); GlobalUnlock(memory);
        if (!OpenClipboard(owner)) throw new Exception("Fixture clipboard busy");
        try { EmptyClipboard(); if (SetClipboardData(format, memory) == IntPtr.Zero) throw new Exception("Fixture copy failed"); }
        finally { CloseClipboard(); }
    }
    static void Pump(int ms) {
        var until = DateTime.UtcNow.AddMilliseconds(ms);
        while (DateTime.UtcNow < until) { Application.DoEvents(); Thread.Sleep(20); }
    }
    static void Report(string phase) {
        using (var pipe = new System.IO.Pipes.NamedPipeClientStream(".", "lossy-" + System.Security.Principal.WindowsIdentity.GetCurrent().User.Value, System.IO.Pipes.PipeDirection.InOut)) {
            pipe.Connect(1500);
            var bytes = System.Text.Encoding.UTF8.GetBytes("{\"op\":\"status\"}");
            pipe.Write(BitConverter.GetBytes(bytes.Length), 0, 4); pipe.Write(bytes, 0, bytes.Length); pipe.Flush();
            var reader = new System.IO.BinaryReader(pipe);
            var length = reader.ReadInt32();
            if (length < 0 || length > 65536) throw new Exception("Unexpected status size");
            Console.WriteLine(phase + ": " + System.Text.Encoding.UTF8.GetString(reader.ReadBytes(length)));
        }
    }
    [STAThread] static void Main() {
        using (var form = new Form { Text = "Lossy synthetic capture verification", Width = 580, Height = 220, TopMost = true })
        using (var box = new TextBox { Multiline = true, Dock = DockStyle.Fill, AccessibleName = "Synthetic message" }) {
            form.Controls.Add(box);
            form.Show(); form.Activate(); box.Focus();
            uint pid;
            uint other = GetWindowThreadProcessId(GetForegroundWindow(), out pid);
            uint own = GetCurrentThreadId();
            AttachThreadInput(own, other, true);
            try { SetForegroundWindow(form.Handle); box.Focus(); }
            finally { AttachThreadInput(own, other, false); }
            Pump(500);
            box.Text = "Synthetic native draft"; Pump(700);
            box.Text = "Synthetic native draft continued"; Pump(700);
            // Explicit owner: ownerless OLE copies cannot be attributed safely.
            CopyData(form.Handle, 13, System.Text.Encoding.Unicode.GetBytes("Synthetic clipboard text\0")); Pump(1800); Report("Text copy");
            using (var bitmap = new Bitmap(80, 60)) {
                using (var graphics = Graphics.FromImage(bitmap)) graphics.Clear(Color.Pink);
                using (var stream = new System.IO.MemoryStream()) {
                    bitmap.Save(stream, System.Drawing.Imaging.ImageFormat.Png);
                    CopyData(form.Handle, RegisterClipboardFormat("PNG"), stream.ToArray());
                }
                Pump(1800); Report("Image copy");
            }
            using (var password = new TextBox { AccessibleName = "Password", UseSystemPasswordChar = true, Dock = DockStyle.Bottom }) {
                form.Controls.Add(password); password.BringToFront(); password.Focus(); Pump(300);
                password.Text = "Synthetic protected field must never persist"; Pump(700);
            }
            form.Close();
        }
    }
}
