//
//  Toolbar.swift
//  Monocurl
//
//  Created by Manu Bhat on 10/28/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

import Foundation
import Cocoa
import SwiftUI
import UniformTypeIdentifiers

fileprivate let timestampFormat = "%02d:%.3f"

fileprivate extension Binding<String> {
    init(digits: Binding<Int>, min: Int? = nil, max: Int? = nil) {
        self.init(get: {
            String(digits.wrappedValue)
        }, set: { str in
            let raw = Int(str.filter {$0.isNumber}) ?? 0
            digits.wrappedValue = Swift.max(Swift.min(raw, max ?? Int.max), min ?? Int.min)
        })
    }
}


fileprivate struct Export: View {
    
    let cache: TimelineCache
    
    @State private var showingExportPopover = false;
    @State private var showingExportScreen = false;
    
    @State private var width: Int = 1960
    @State private var height: Int = 1080
    @State private var fps: Int = 60
    //file type will be done later
    @State private var fileType: String = "mp4"
    @State private var save: URL? = nil;
    @State private var popoverError: String? = nil;
    
    @State private var finishError: String? = nil;
    @State private var finishSuccess = false;
    
    //stateobject so that it's not reinitialize1ed every frame...
    @StateObject private var exporter: ExportController = ExportController();
    
    var body: some View {
        Button {
            showingExportPopover = true;
            popoverError = nil;
        } label: {
            Image(systemName: "square.and.arrow.up")
        }
            .buttonStyle(.plain)
            .padding(.trailing, 15)
            .padding(.bottom, 3)
            .onAppear {
                exporter.successMessage = $finishSuccess;
                exporter.errorMessage = $finishError
            }
            .sheet(isPresented: $showingExportPopover) {
                VStack {
                    Text("Export Config")
                        
                    //width
                    Group {
                        HStack {
                            Text("Width")
                            TextField("Width", text: .init(digits: $width, min: 2))
                        }
                        
                        HStack {
                            Text("Height")
                            TextField("Height", text: .init(digits: $height, min: 2))
                        }
                        
                        HStack {
                            Text("FPS")
                            TextField("FPS", text: .init(digits: $fps, min: 0, max: 240))
                        }
                        
                        Picker("File Type", selection: self.$fileType) {
                            Text("mp4").tag("mp4")
                        }
                        
                        HStack {
                            Button("Choose export location") {
                                let folderPicker = NSSavePanel();
                                folderPicker.allowedContentTypes = [UTType(filenameExtension: self.fileType)!]
                                let res = folderPicker.runModal()
                                
                                if (res == .OK) {
                                    self.save = folderPicker.url
                                }
                            }
                            Text(self.save?.lastPathComponent ?? "No file chosen")
                        }.padding()
                        
                    }.textFieldStyle(.roundedBorder)
                        .frame(width: 300)

                    if let error = self.popoverError {
                        Text("Error: " + error)
                            .foregroundColor(.red)
                    }
                   
                    HStack {
                        Button("Cancel") {
                            showingExportPopover = false
                        }
                        
                        Button("Export") {
                            guard self.width > 0 && self.width % 2 == 0 else {
                                self.popoverError = "Expected even positive width"
                                return
                            }
                            
                            guard self.height > 0 && self.width % 2 == 0 else {
                                self.popoverError = "Expected even positive height"
                                return
                            }
                            
                            guard self.fps > 0 else {
                                self.popoverError = "Expected positive FPS"
                                return
                            }
                            
                            guard let save = self.save else {
                                self.popoverError = "Expected save location"
                                return
                            }
                            
                            showingExportPopover = false;
                            showingExportScreen = true;
                            finishError = nil
                            finishSuccess = false;
                            
                            let cache = ViewportCache(self.cache.ref.pointee.handle.pointee.viewport, nonce: 0)
                            exporter.export(gpu: AppDelegate.gpu, cache: cache, width: width, height: height, fps: fps, upf: 1, save: save)
                        }
                    }
                    .padding()
                    
                }.padding()
            }
            .sheet(isPresented: $showingExportScreen) {
                VStack {
                    Text("Exporting")
                    
                    if finishSuccess {
                        Text("Success")
                            .foregroundColor(.blue)
                            .frame(width: 300)
                        
                        HStack {
                            Button("Exit") {
                                showingExportScreen = false;
                            }
                        }
                        .onAppear {
                            if let save = self.save {
                                NSWorkspace.shared.open(save)
                            }
                        }
                    }
                    else if let error = finishError {
                        Text("Error: " + error)
                            .foregroundColor(.red)
                            .frame(width: 300)

                        Button("Exit") {
                            showingExportScreen = false;
                        }
                    }
                    else {
                        HStack {
                            Text(String(format: timestampFormat, self.cache.timestamp.slide, self.cache.timestamp.offset))
                                .font(.body.monospacedDigit())
                                .frame(width: 100)
                            ProgressView(value: Float(self.cache.timestamp.slide) / Float(self.cache.slides.count))
                                .frame(width: 300)
                        }.padding()
                        
                        Button("Cancel") {
                            //send a cancel message
                            timeline_interrupt_export(self.cache.ref)
                        }
                    }
                }.padding()
            }
    }
}

struct Toolbar: View {
    @Binding var cache: TimelineCache
    @EnvironmentObject var environment: MonocurlEnvironment

    var body: some View {
        HStack(spacing: 0) {
            Spacer()
           
            Group {
                Button {
                    timeline_seek_to(self.cache.ref, timestamp(slide: 0, offset: 0), 1)
                } label: {
                    Image(systemName: "backward.end")
                }.buttonStyle(.link)
                    .padding(.trailing, 3)
                
                Button {
                    let timeline = self.cache.ref
                    timeline_seek_to(timeline, timestamp(slide: self.cache.timestamp.slide, offset: 0), 1)
                } label: {
                    Image(systemName: "backward.frame")
                }.buttonStyle(.link)
                    .padding(.trailing, 3)
                
                Button {
                    timeline_play_toggle(self.cache.ref)
                } label: {
                    Image(systemName: self.cache.isPlaying ? "pause" : "play")
                }.buttonStyle(.link)
                    .padding(.trailing, 5)
                
            }.frame(width: 15)
                
            
            Text(String(format: timestampFormat, self.cache.timestamp.slide, self.cache.timestamp.offset))
                .font(.system(size: 15).monospacedDigit())
                    .padding(.vertical, 2)
                    .frame(width: 100, alignment: .leading)
            
            Spacer()
           
            if !environment.inPresentationMode {
                Export(cache: cache)
            }
        }
            .frame(maxWidth: .infinity)
            .background(Color.black)
     
    }
}
