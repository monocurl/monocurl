//
//  Media.swift
//  Monocurl
//
//  Created by Manu Bhat on 9/11/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

import SwiftUI
import UniformTypeIdentifiers



fileprivate func requestImageURL() -> URL? {
    let folderPicker = NSOpenPanel();
    folderPicker.allowedContentTypes = [UTType.image]
    let res = folderPicker.runModal()
    
    if (res == .OK) {
        return folderPicker.url;
    }
    
    return nil;
}


struct MediaItem: View {
    @ObservedObject var cache: EditingMediaCache
    
    @State private var isShowingEdit = false;
    @State private var nameBuffer: String;
    @State private var urlBuffer: URL!
    
    init(cache: EditingMediaCache) {
        self._cache = ObservedObject(wrappedValue: cache)
        self._nameBuffer = State(initialValue: cache.name);
        self._urlBuffer = State(initialValue: cache.path)
    }
    
    var imageName: String {
        switch (cache.type) {
        case RAW_MEDIA_IMAGE:
            return "photo"
        default:
            return "photo"
        }
    }
    
    var body: some View {
        HStack {
            Image(systemName: self.imageName)
                .resizable()
                .scaledToFit()
                .frame(width: 25, height: 25)
                .padding(.leading, 10)
                .foregroundColor(.yellow)
            
            Divider()
            
            VStack(alignment: .leading) {
                //type, name, location (or relink)?
                HStack {
                    Text(cache.name).font(.title3)
                    
                    Spacer()
                    Group {
                        Button {
                            self.isShowingEdit = true;
                        } label: {
                            Image(systemName: "pencil")
                        }
                        
                        Button {
                            media_delete(cache.ref)
                        } label: {
                            Image(systemName: "trash")
                        }
                    }.buttonStyle(.plain)
                }
                Text(cache.path?.relativePath ?? "")
            }
        }.sheet(isPresented: $isShowingEdit) {
            VStack {
                Text("Edit Media")
                
                Group {
                    HStack {
                        Text("Name")
                        TextField("name", text: $nameBuffer)
                    }
                    
                    HStack {
                        Button("Relink") {
                            guard let url = requestImageURL() else {
                                return
                            }
                            self.urlBuffer = url;
                        }
                        Text(urlBuffer == nil ? "" : path(for: self.urlBuffer))
                    }
                }
                
                HStack {
                    Button("Update") {
                        guard let url = self.urlBuffer else {
                            return
                        }
                        guard let bookmark = bookmark(for: url)?.base64EncodedString() else {
                            return
                        }
                        
                        let main = self.nameBuffer.utf8CStringPointer
                        let dup = strdup(main)
                        main.deallocate()
                        
                        media_switch_name(self.cache.ref, dup);
                        media_switch_path(self.cache.ref, bookmark.utf8CStringPointer);
                        self.isShowingEdit = false;
                    }
                    
                    Button("Cancel") {
                        self.isShowingEdit = false;
                    }
                }.padding()
            }.padding()
                .frame(minWidth: 300)
        }
    }
}

struct Media: View {
    @ObservedObject var state: EditingSceneCache
    
    var body: some View {
        ScrollView {
            VStack {
                //of media items
                ForEach(state.media) { media in
                    MediaItem(cache: media)
                }
                
                Button {
                    guard let url = requestImageURL() else {
                        return
                    }
                    
                    let bookmark = bookmark(for: url).base64EncodedString()
                    let name = url.lastPathComponent;

                    media_insert_image(state.ref, name.utf8CStringPointer, bookmark.utf8CStringPointer);
                } label: {
                    Image(systemName: "plus.app")
                }
                
                Spacer()
                    .frame(maxHeight: .infinity)
            }
            .frame(maxWidth: .infinity)
            
        }.frame(maxWidth: .infinity)
    }
}
