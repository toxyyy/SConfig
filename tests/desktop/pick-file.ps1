param([uint32]$AppProcess,[string]$SelectedPath)
$ErrorActionPreference='Stop'
Add-Type @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class FixtureDialog {
    public delegate bool Visit(IntPtr h,IntPtr data);
    [DllImport("user32")] static extern bool EnumWindows(Visit fn,IntPtr data);
    [DllImport("user32")] static extern bool EnumChildWindows(IntPtr h,Visit fn,IntPtr data);
    [DllImport("user32")] static extern uint GetWindowThreadProcessId(IntPtr h,out uint pid);
    [DllImport("user32",CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr h,StringBuilder s,int len);
    [DllImport("user32")] static extern int GetDlgCtrlID(IntPtr h);
    [DllImport("user32")] static extern IntPtr GetParent(IntPtr h);
    [DllImport("user32",CharSet=CharSet.Unicode)] static extern IntPtr SendMessage(IntPtr h,int m,IntPtr w,string l);
    [DllImport("user32")] static extern bool PostMessage(IntPtr h,int m,IntPtr w,IntPtr l);
    static string Class(IntPtr h) {var s=new StringBuilder(128); GetClassName(h,s,128);return s.ToString();}
    public static bool Choose(uint id,string file) {
        IntPtr dialog=IntPtr.Zero,edit=IntPtr.Zero;
        EnumWindows((h,d)=>{uint pid; GetWindowThreadProcessId(h,out pid);if(pid==id && Class(h)=="#32770"){dialog=h;return false;}return true;},IntPtr.Zero);
        if(dialog==IntPtr.Zero)return false;
        var edits=new List<IntPtr>();
        EnumChildWindows(dialog,(h,d)=>{if(Class(h)=="Edit")edits.Add(h);return true;},IntPtr.Zero);
        edit=edits.Find(h=>GetDlgCtrlID(h)==1148);
        if(edit==IntPtr.Zero)edit=edits.Find(h=>GetDlgCtrlID(GetParent(h))==1148);
        if(edit==IntPtr.Zero)edit=edits.Find(h=>GetDlgCtrlID(GetParent(GetParent(h)))==1148);
        if(edit==IntPtr.Zero)edit=edits.Find(h=>GetDlgCtrlID(h)==1001);
        if(edit==IntPtr.Zero)edit=edits.Find(h=>GetDlgCtrlID(GetParent(h))==1148);
        if(edit==IntPtr.Zero)edit=edits.Find(h=>Class(GetParent(h))=="ComboBox");
        if(edit==IntPtr.Zero)return false;
        SendMessage(edit,12,IntPtr.Zero,file);
        System.Threading.Thread.Sleep(180);
        PostMessage(dialog,273,(IntPtr)1,IntPtr.Zero);
        return true;
    }
}
'@
for($i=0;$i -lt 60;$i++){if([FixtureDialog]::Choose($AppProcess,$SelectedPath)){exit 0};Start-Sleep -Milliseconds 125}
throw 'No native filename edit in the test process.'

