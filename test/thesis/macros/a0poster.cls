%% 
%% This is file `a0poster.cls'
%% 
%% Copyright (C) 1997-2004 Gerlinde Kettl and Matthias Weiser
%%
%% It may be distributed and/or modified under the
%% conditions of the LaTeX Project Public License, either version 1.2
%% of this license or (at your option) any later version.
%% The latest version of this license is in
%%    http://www.latex-project.org/lppl.txt
%% and version 1.2 or later is part of all distributions of LaTeX
%% version 1999/12/01 or later.
%%
%% Problems, bugs and comments to 
%% tex@kettl.de
%% 

\ProvidesClass{a0poster}[2004/01/31 v1.22b a0poster class (GK, MW)]
\NeedsTeXFormat{LaTeX2e}[1995/06/01]
\LoadClass{article}

\newif\ifportrait
\newif\ifanullb
\newif\ifanull
\newif\ifaeins
\newif\ifazwei
\newif\ifadrei
\newif\ifposterdraft

\newcount\xkoord
\newcount\ykoord
\newcount\xscale
\newcount\yscale

\DeclareOption{a0b}{
\anullbtrue
\xkoord=2594 % big points (1 bp=1/72 inch)
\ykoord=3370 % big points (1 bp=1/72 inch)
\xscale=2
\yscale=2
}
\DeclareOption{a0}{
\anulltrue\anullbfalse
\xkoord=2380 % big points (1 bp=1/72 inch)
\ykoord=3368 % big points (1 bp=1/72 inch)
\xscale=2
\yscale=2
}
\DeclareOption{a1}{
\aeinstrue\anullbfalse
\xkoord=1684 % big points (1 bp=1/72 inch)
\ykoord=2380 % big points (1 bp=1/72 inch)
\xscale=3
\yscale=3
}
\DeclareOption{a2}{
\azweitrue\anullbfalse
\xkoord=1190 % big points (1 bp=1/72 inch)
\ykoord=1684 % big points (1 bp=1/72 inch)
\xscale=4
\yscale=4

}
\DeclareOption{a3}{
\adreitrue\anullbfalse
\xkoord=842  % big points (1 bp=1/72 inch)
\ykoord=1190 % big points (1 bp=1/72 inch)
\xscale=6
\yscale=6
}
\DeclareOption{landscape}{
\portraitfalse
}
\DeclareOption{portrait}{
\portraittrue
}
\DeclareOption{draft}{   %% Just for downward compatibility
\posterdrafttrue
}
\DeclareOption{posterdraft}{ %% To avoid collisions with the graphics package
\posterdrafttrue
}
\DeclareOption{final}{
\posterdraftfalse
}
\DeclareOption*{\PackageWarning{a0poster}{Unknown Option \CurrentOption}}
\ExecuteOptions{landscape,a0b,final}
\ProcessOptions\relax

\ifanullb
   \setlength{\paperwidth}{119cm}
   \setlength{\paperheight}{87cm}
   \setlength{\textwidth}{114cm}
   \setlength{\textheight}{87cm}
\else\ifanull
        \setlength{\paperwidth}{118.82cm}
        \setlength{\paperheight}{83.96cm}
        \setlength{\textwidth}{114.82cm}
        \setlength{\textheight}{79.96cm}
     \else\ifaeins
             \setlength{\paperwidth}{83.96cm}
             \setlength{\paperheight}{59.4cm}
             \setlength{\textwidth}{79.96cm}
             \setlength{\textheight}{55.4cm}
          \else\ifazwei
                  \setlength{\paperwidth}{59.4cm}
                  \setlength{\paperheight}{41.98cm}
                  \setlength{\textwidth}{55.4cm}
                  \setlength{\textheight}{37.98cm}
               \else\ifadrei
                       \setlength{\paperwidth}{41.98cm}
                       \setlength{\paperheight}{29.7cm}
                       \setlength{\textwidth}{37.98cm}
                       \setlength{\textheight}{25.7cm}
                    \else\relax
                    \fi
               \fi
          \fi
     \fi
\fi


\ifportrait
   \newdimen\tausch
   \setlength{\tausch}{\paperwidth}
   \setlength{\paperwidth}{\paperheight}
   \setlength{\paperheight}{\tausch}
   \setlength{\tausch}{\textwidth}
   \setlength{\textwidth}{\textheight}
   \setlength{\textheight}{\tausch}
\else\relax
\fi

%% Setting proper dimensions for a DIN A0 printer

\setlength{\headheight}{0 cm}
\setlength{\headsep}{0 cm}
\setlength{\topmargin}{0 cm}
\setlength{\oddsidemargin}{0 cm}

\catcode`\%=11
\newwrite\Ausgabe
\immediate\openout\Ausgabe=a0header.ps
\write\Ausgabe{%%BeginFeature *PageSize ISOA0/ISO A0}
\ifposterdraft\write\Ausgabe{2 dict dup /PageSize [595 842] put dup /ImagingBBox null put}
\else\write\Ausgabe{2 dict dup /PageSize [\number\xkoord\space \number\ykoord] put dup /ImagingBBox null put}\fi
\write\Ausgabe{setpagedevice}
\ifposterdraft\write\Ausgabe{38 84 translate}
\write\Ausgabe{0.\number\xscale\space 0.\number\yscale\space scale}\else\relax\fi
\write\Ausgabe{%%EndFeature}
\closeout\Ausgabe
\catcode`\%=14

\special{header=./a0header.ps}

\input{a0size.sty}

\renewcommand{\tiny}{\fontsize{12}{14}\selectfont}
\renewcommand{\scriptsize}{\fontsize{14.4}{18}\selectfont}   
\renewcommand{\footnotesize}{\fontsize{17.28}{22}\selectfont}
\renewcommand{\small}{\fontsize{20.74}{25}\selectfont}
\renewcommand{\normalsize}{\fontsize{24.88}{30}\selectfont}
\renewcommand{\large}{\fontsize{29.86}{37}\selectfont}
\renewcommand{\Large}{\fontsize{35.83}{45}\selectfont}
\renewcommand{\LARGE}{\fontsize{43}{54}\selectfont}
\renewcommand{\huge}{\fontsize{51.6}{64}\selectfont}
\renewcommand{\Huge}{\fontsize{61.92}{77}\selectfont}
\newcommand{\veryHuge}{\fontsize{74.3}{93}\selectfont}
\newcommand{\VeryHuge}{\fontsize{89.16}{112}\selectfont}
\newcommand{\VERYHuge}{\fontsize{107}{134}\selectfont}

\setlength\smallskipamount{6pt plus 2pt minus 2pt}
\setlength\medskipamount{12pt plus 4pt minus 4pt}
\setlength\bigskipamount{24pt plus 8pt minus 8pt}

\setlength\abovecaptionskip{25pt}
\setlength\belowcaptionskip{0pt}
\setlength\abovedisplayskip{25pt plus 6pt minus 15 pt}
\setlength\abovedisplayshortskip{0pt plus 6pt}
\setlength\belowdisplayshortskip{13pt plus 7pt minus 6pt}
\setlength\belowdisplayskip\abovedisplayskip

\normalsize

%%Florian Rabe's extensions; based on macros by Christoph Lange
% tikz with some arrorws, colors, and default style
\usepackage{tikz}
\usepackage{pgflibraryarrows}
\definecolor{light-yellow}{rgb}{1,1,0.8} 
\definecolor{dark-blue}{rgb}{0.1,0,0.7}
\tikzstyle{every picture}=[scale=3,line width=3]
\tikzstyle{default}=[very thick,font=\sffamily,>=triangle 60]
% font and other styles
\def\defaulttextfont{\Large}
\setlength{\columnsep}{1.9cm}
\setlength{\columnseprule}{1pt}
% boxes for title, abstract etc.
\newenvironment{authorbox}[1]{\begin{minipage}[c]{#1}\centering\Large\bf}{\end{minipage}}
\newcommand{\logobox}[2]{\begin{minipage}[c]{#1}\centering\includegraphics[width=\textwidth]{#2}\end{minipage}}
\newcommand{\yellowbox}[1]{%
    \renewcommand{\rmdefault}{lmr}%
    \setlength{\fboxrule}{0.1cm}%
    \setlength{\fboxsep}{.5cm}%
    {\begin{center}%
    \fcolorbox{black}{light-yellow}%
    {\parbox{0.99\textwidth}%
    {\vspace{0.5cm} \bf \Large #1
    \vspace{0.5cm}}}\end{center}}%
    \renewcommand{\rmdefault}{ptm}}
\newcommand{\colored}[1]{\textcolor{DarkRed}{#1}}
%macros for titlepage
\newcommand{\poster@title}{}
\renewcommand{\title}[1]{\renewcommand{\poster@title}{#1}}
\newcommand{\poster@author}{}
\renewcommand{\author}[1]{\renewcommand{\poster@author}{#1}}
\newcommand{\poster@institute}{}
\newcommand{\institute}[1]{\renewcommand{\poster@institute}{#1}}
\newcommand{\poster@email}{}
\newcommand{\email}[1]{\renewcommand{\poster@email}{#1}}
\newcommand{\poster@abstract}{}
\renewcommand{\abstract}[1]{\renewcommand{\poster@abstract}{#1}}
\renewcommand{\titlepage}{
\begin{center}
  \begin{minipage}[c]{0.15\textwidth}\centering\includegraphics[width=\textwidth]{images/Jacobs_LOGO_RGB}\end{minipage}%
  \begin{minipage}[c]{0.7\textwidth} 
    \begin{center}
      {\veryHuge \bf
        \begin{tabular}[t]{c}
          \poster@title
        \end{tabular}}\\
      \vspace{1cm}
      \begin{authorbox}{0.7\textwidth}
        \poster@author \hspace{1cm} \texttt{<\poster@email>}\\
        \poster@institute
      \end{authorbox}
    \end{center}
\end{minipage}%
  % SECOND LOGO
  \begin{minipage}[c]{0.15\textwidth}\centering\includegraphics[width=.7\textwidth]{images/kwarc}\end{minipage}%
\end{center}
\vspace{0.5cm} \yellowbox{\poster@abstract}
\vspace{0.5cm} \defaulttextfont
}
%(re)define beamer macros so that slides can be copied into poster as is
\renewenvironment{frame}{}{\medskip}
\newcommand{\frametitle}[1]{\begin{center}\bf #1\end{center}\medskip}
%%end extensions

\AtBeginDocument{
\ifanullb
   \ifportrait\special{papersize=91.51cm,120cm}\else\special{papersize=120cm,91.51cm}\fi
\else\ifanull
        \ifportrait\special{papersize=83.96cm,118.82cm}\else\special{papersize=118.82cm,83.96cm}\fi
     \else\ifaeins
             \ifportrait\special{papersize=59.4cm,83.96cm}\else\special{papersize=83.96cm,59.4cm}\fi
          \else\ifazwei
                  \ifportrait\special{papersize=41.98cm,59.4cm}\else\special{papersize=59.4cm,41.98cm}\fi
               \else\ifadrei
                      \ifposterdraft
                       \ifportrait\special{papersize=29.7cm,41.35cm}\else\special{papersize=41.98cm,29.0cm}\fi
                      \else
                       \ifportrait\special{papersize=29.7cm,41.98cm}\else\special{papersize=41.98cm,29.7cm}\fi
                      \fi
                    \else\relax
                    \fi
               \fi
          \fi
     \fi
\fi
\pagestyle{empty}
}

\endinput
%% 
%% End of file `a0poster.cls'.
