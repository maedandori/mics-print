param(
 [Parameter(Mandatory=$true)][string]$JobFile,
 [Parameter(Mandatory=$true)][string]$Template,
 [Parameter(Mandatory=$true)][string]$Output
)
$ErrorActionPreference='Stop'
$excel=$null; $book=$null; $base=$null
function Release-Com($obj) { if($null -ne $obj) { try { [void][System.Runtime.InteropServices.Marshal]::FinalReleaseComObject($obj) } catch {} } }
try {
 $job=Get-Content -LiteralPath $JobFile -Raw -Encoding UTF8 | ConvertFrom-Json
 if(-not (Test-Path -LiteralPath $Template -PathType Leaf)) {throw "Không tìm thấy XLSX: $Template"}
 $dir=[System.IO.Path]::GetDirectoryName($Output)
 [System.IO.Directory]::CreateDirectory($dir) | Out-Null
 # Chỉ tạo file mới, tuyệt đối không xóa PDF cũ đang được trình đọc/OneDrive hoặc SumatraPDF sử dụng.
 if(Test-Path -LiteralPath $Output) {throw "Tệp PDF đầu ra đã tồn tại; yêu cầu tên file duy nhất: $Output"}
 $excel=New-Object -ComObject Excel.Application
 $excel.Visible=$false;$excel.DisplayAlerts=$false;$excel.AutomationSecurity=3
 $book=$excel.Workbooks.Open($Template,0,$true)
 $base=$book.Worksheets.Item('原紙')
 $names=@('B8','H8','N8','T8','B32','H32','N32','T32')
 $colors=@('B12','H12','N12','T12','B36','H36','N36','T36')
 $quantities=@('B16','H16','N16','T16','B40','H40','N40','T40')
 $numbers=@('E20','K20','Q20','W20','E44','K44','Q44','W44')
 $cards=@($job.cards)
 $pages=[Math]::Max(1,[int][Math]::Ceiling($cards.Count/8.0))
 for($page=0;$page -lt $pages;$page++){
  $sheet=$null
  try {
   $base.Copy([Type]::Missing,$book.Worksheets.Item($book.Worksheets.Count))
   $sheet=$book.Worksheets.Item($book.Worksheets.Count)
   for($i=0;$i -lt 8;$i++){
    $index=$page*8+$i
    $card=if($index -lt $cards.Count){$cards[$index]}else{$null}
    $name=if($null -ne $card){[string]$card.name}else{''}
    $color=if($null -ne $card){[string]$card.color}else{''}
    $qty=if($null -ne $card){[string]$card.quantity}else{''}
    $number=''
    if($null -ne $card) { $number=[string]([int]$job.startNumber+$index); if($card.isFinal){$number+='(完)'} }
    $sheet.Range($names[$i]).Value2=$name
    $sheet.Range($colors[$i]).Value2=$color
    $sheet.Range($quantities[$i]).Value2=$qty
    $sheet.Range($numbers[$i]).Value2=$number
   }
   if($null -ne $job.cells){
    foreach($property in $job.cells.PSObject.Properties){
     $location=[string]$property.Name
     $address=($location -split '!')[-1]
     $sheetName=if($location.Contains('!')){($location -split '!')[0].Trim("' ")}else{''}
     if($address -notmatch '^\$?[A-Z]{1,3}\$?[1-9][0-9]{0,6}$' -or ($sheetName -ne '' -and $sheetName -ne '原紙')){throw "Địa chỉ ô không hợp lệ: $location"}
     $value=if($null -eq $property.Value){''}else{[string]$property.Value}
     if($value.Length -gt 1000){throw "Ô quá dài: $address"}
     $sheet.Range($address).Value2=$value
    }
   }
  } finally {Release-Com $sheet}
 }
 $base.Delete()
 $book.ExportAsFixedFormat(0,$Output,0,$true,$false)
 if(-not (Test-Path -LiteralPath $Output -PathType Leaf)){throw "Excel chưa tạo PDF"}
 $stream=[System.IO.File]::OpenRead($Output)
 try{$header=New-Object byte[] 5;[void]$stream.Read($header,0,5);if([System.Text.Encoding]::ASCII.GetString($header) -ne '%PDF-'){throw 'Sai định dạng PDF'}}finally{$stream.Dispose()}
 Write-Output "PDF_OK: $Output"
}
catch { Write-Error $_; exit 1 }
finally {
 if($null -ne $book){try{$book.Close($false)}catch{}}
 if($null -ne $excel){try{$excel.Quit()}catch{}}
 Release-Com $base
 Release-Com $book
 Release-Com $excel
 [GC]::Collect();[GC]::WaitForPendingFinalizers()
}
