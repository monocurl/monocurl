//
//  Landing.swift
//  Monocurl
//
//  Created by Manu Bhat on 9/6/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

import Foundation
import SwiftUI
import UniformTypeIdentifiers

struct LandingPage: View {
    //you can create either image or a scene
    @EnvironmentObject private var storage: StorageEnvironment
    
    @State var deletionURL: URL?
    @State var deleting = false
    @Binding var url: URL?
 
    var controls: some View {
        HStack {
            //create scene or image (or import)
            Text("Projects")
                .font(.largeTitle)
                .padding(.trailing, 5)
            
            Group {
                Button("New Scene") {
                    self.storage.addUrl(AppRoot.sceneType) { scene in
                        let ptr = path(for: scene).utf8CStringPointer;
                        file_write_default_scene(ptr);
                        
                        ptr.deallocate();
                    }
                }
                .buttonStyle(.link)
                
                Button("Import") {
                    self.storage.tryImport();
                }
                .buttonStyle(.link)
            }
        }
        .padding(.top)
        .frame(alignment: .center)
    }
    
    func project(url: URL) -> some View {
        HStack {
            Button {
                deleting = true
                deletionURL = url
            } label: {
                Image(systemName: "trash")
            }
            .buttonStyle(.link)
            
            Button {
                self.url = url
                self.storage.makeFirst(url: url)
            } label: {
                HStack {
                    Text((url.lastPathComponent as NSString).deletingPathExtension)
                        .font(.title2)
                }
            }
            .buttonStyle(.plain)
            
            Spacer()

        }
        .padding(.horizontal, 20)
        .padding(.vertical, 2)
    }
    
    var projects: some View {
        Group {
            if (self.storage.bookmarks.count == 0) {
                Text("No active projects. Create a scene to get started! Also, only import projects you trust!")
            }
            else {
                VStack {
                    ForEach(self.storage.urls, id: \.self) { url in
                        self.project(url: url)
                    }
                }
            }
        }
    }
    
    var logo: some View {
        VStack {
            Image("monocurl-1024", bundle: nil)
                .resizable()
                .frame(width: 500, height: 500)
            
            Group {
                HStack {
                    Text("Monocurl")
                        .font(.title)
                    
                    Text("[version " + String(cString: monocurl_version_str()) + " - beta]")
                }
               
                HStack {
                    Link("Website", destination: URL(string: "https://www.monocurl.com")!)
                    Link("GitHub", destination: URL(string: "https://www.github.com/monocurl/monocurl")!)
                    Link("Discord", destination: URL(string: "https://discord.gg/7g94JR3SAD")!)
                }
                .padding(.bottom, 100)
                
                Text("Remember that Monocurl may not be forwards/backwards compatibile during beta! Also, only open projects you trust.")
                    .font(.caption)
                    .multilineTextAlignment(.center)
            }
            .frame(maxWidth: 300, alignment: .center)
        }
        .frame(minWidth: 600, maxHeight: .infinity)
        .background(Color.black)
    }
    
    var body: some View {
        HStack(spacing: 0) {
            self.logo
            
            Rectangle()
                .fill(.purple.opacity(0.2))
                .frame(maxWidth: 1, maxHeight: .infinity)
                    
            
            VStack {
                self.controls
                
                Rectangle()
                    .fill(.purple.opacity(0.1))
                    .frame(maxWidth: .infinity, maxHeight: 1)
                
                ScrollView {
                    self.projects
                        .padding(.bottom, 300)
                }
            }
            .background(Color(red: 0.07, green: 0.07, blue: 0.07))
            
        }
        .background(Color.black)
        .sheet(isPresented: $deleting) {
            VStack {
                Text("Confirm Deletion?")
                
                HStack {
                    Button("Delete") {
                        self.storage.remove(url: self.deletionURL!)
                        self.deleting = false
                        self.deletionURL = nil
                    }
                    
                    Button("Cancel") {
                        self.deleting = false
                        self.deletionURL = nil
                    }
                }
                .padding()
            }
            .padding()
        }
    }
}
