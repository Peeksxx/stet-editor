function ExpandEnvironmentStrings(Source, Destination: String; Size: Cardinal): Cardinal;
  external 'ExpandEnvironmentStringsW@kernel32.dll stdcall';

function NormalizeEntry(Value: String): String;
var
  Required: Cardinal;
  Expanded: String;
begin
  Result := Trim(Value);
  if Pos('%', Result) > 0 then begin
    Required := ExpandEnvironmentStrings(Result, '', 0);
    if (Required > 0) and (Required <= 32768) then begin
      SetLength(Expanded, Required);
      if ExpandEnvironmentStrings(Result, Expanded, Required) = Required then begin
        SetLength(Expanded, Required - 1);
        Result := Trim(Expanded);
      end;
    end;
  end;
  if (Length(Result) > 1) and (Result[1] = '"') and (Result[Length(Result)] = '"') then
    Result := Copy(Result, 2, Length(Result) - 2);
  StringChangeEx(Result, '/', '\', True);
  while (Length(Result) > 3) and (Result[Length(Result)] = '\') do
    Delete(Result, Length(Result), 1);
  Result := Lowercase(Result);
end;

function ContainsPath(Value, Target: String): Boolean;
var
  Entry: String;
  Separator: Integer;
begin
  Result := False;
  Target := NormalizeEntry(Target);
  if Target = '' then Exit;
  repeat
    Separator := Pos(';', Value);
    if Separator = 0 then begin
      Entry := Value;
      Value := '';
    end else begin
      Entry := Copy(Value, 1, Separator - 1);
      Delete(Value, 1, Separator);
    end;
    if NormalizeEntry(Entry) = Target then begin
      Result := True;
      Exit;
    end;
  until Separator = 0;
end;

function AppendPathEntry(Value, Target: String): String;
begin
  Result := Value;
  if (NormalizeEntry(Target) = '') or ContainsPath(Value, Target) then Exit;
  if Value <> '' then Result := Value + ';';
  Result := Result + Target;
end;

function RemovePathEntry(Value, Target: String): String;
var
  Entry: String;
  Separator: Integer;
  First: Boolean;
begin
  Result := Value;
  Target := NormalizeEntry(Target);
  if (Target = '') or not ContainsPath(Value, Target) then Exit;
  Result := '';
  First := True;
  repeat
    Separator := Pos(';', Value);
    if Separator = 0 then begin
      Entry := Value;
      Value := '';
    end else begin
      Entry := Copy(Value, 1, Separator - 1);
      Delete(Value, 1, Separator);
    end;
    if NormalizeEntry(Entry) <> Target then begin
      if not First then Result := Result + ';';
      Result := Result + Entry;
      First := False;
    end;
  until Separator = 0;
end;
