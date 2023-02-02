<?php

namespace App\Presentation\Controller;

use Symfony\Bundle\FrameworkBundle\Controller\AbstractController;
use Symfony\Component\HttpFoundation\Response;
use Symfony\Component\Routing\Annotation\Route;

class BaseVueController extends AbstractController
{
    #[Route("/app/",    name: "vue_home",    methods: ["GET"])]
    public function renderBaseTemplate(): Response
    {
        return $this->render('vue.html.twig');
    }
}
