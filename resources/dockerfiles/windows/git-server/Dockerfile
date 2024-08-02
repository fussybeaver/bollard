FROM mcr.microsoft.com/windows/nanoserver@sha256:8f78a4a7da4464973a5cd239732626141aec97e69ba3e4023357628630bc1ee2

# configure powershell
SHELL ["powershell", "-Command", "$ErrorActionPreference = 'Stop'; $ProgressPreference = 'SilentlyContinue';"] 
RUN Set-ExecutionPolicy -Scope CurrentUser -ExecutionPolicy RemoteSigned

RUN Invoke-WebRequest -URI 'https://github.com/git-for-windows/git/releases/download/v2.46.0.windows.1/MinGit-2.46.0-64-bit.zip' -OutFile 'C:\\MinGit.zip'

RUN Expand-Archive c:\\MinGit.zip -DestinationPath C:\\git
RUN setx /M PATH $($Env:PATH + ';C:\\git\\cmd;C:\\git\\bin;C:\\git\\usr\\bin')

RUN Invoke-WebRequest -URI 'https://github.com/PowerShell/Win32-OpenSSH/releases/download/v9.5.0.0p1-Beta/OpenSSH-Win64.zip' -OutFile 'C:\\openssh.zip'

RUN Expand-Archive c:\\openssh.zip -DestinationPath C:/

WORKDIR C:/OpenSSH-Win64/

SHELL ["cmd.exe"] 

RUN cd C:/OpenSSH-Win64/ && c:/OpenSSH-Win64/ssh-keygen.exe -t ecdsa -N '""' -f ssh_host_ecdsa_key

SHELL ["powershell", "-Command", "$ErrorActionPreference = 'Stop'; $ProgressPreference = 'SilentlyContinue';"] 

RUN net USER ssh "Passw0rd" /ADD 
RUN net localgroup "Administrators" "ssh" /ADD

RUN New-Item -Path HKLM:\SOFTWARE -Name OpenSSH -Force
RUN New-ItemProperty -Path HKLM:\SOFTWARE\OpenSSH -Name DefaultShell -Value c:\ps7\pwsh.exe -PropertyType string -Force

RUN ./Install-sshd.ps1; ./FixHostFilePermissions.ps1 -Confirm:$false;

RUN ls C:/OpenSSH-Win64

EXPOSE 22

