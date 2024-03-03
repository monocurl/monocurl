//
//  Util.swift
//  Monocurl
//
//  Created by Manu Bhat on 11/10/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

import Foundation


extension String {
    var utf8CStringPointer: UnsafeMutablePointer<CChar> {
        let cString = self.utf8CString;
        let pointer: UnsafeMutablePointer<CChar> = .allocate(capacity: cString.count)
        
        cString.withContiguousStorageIfAvailable { source in
            let _ = memcpy(pointer, source.baseAddress, source.count);
        }
        
        return pointer;
    }
}

func path(for url: URL) -> String {
    if #available(macOS 13.0, *) {
        return url.path(percentEncoded: false)
    }
    else {
        return url.path
    }
}

//base 64 encoded
func bookmark(for url: URL) -> Data! {
    let url = url as NSURL
    do {
        url.startAccessingSecurityScopedResource()
        let data = try url.bookmarkData(options: .withSecurityScope, includingResourceValuesForKeys: nil, relativeTo: nil);
        url.stopAccessingSecurityScopedResource()
        return data;
    } catch let error {
        NSLog("Error creating bookmark! \(error)");
    }
    
    return nil;
}
